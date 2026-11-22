//! The hires (ONBOARD-T1): who is contracted, under which name and avatar, and
//! whether the onboarding is done. The store applies the rules here: one active
//! hire per role, eight active hires per area, and a team that is contracted
//! whole or not at all. Every write runs under the store lock, so a check and
//! its write cannot interleave with another writer.

use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::HireError;
use crate::id;
use crate::model_hire::{
    Hire, HireFilter, HireOutcome, HireStatus, NewHire, OnboardingState, MAX_HIRES_PER_AREA,
};
use crate::schema;
use crate::text::agent_name;

use super::company_id;

const HIRE_COLUMNS: &str = "id, catalog_role, area, name, avatar, engine, profile, \
     skills_allow, status, created_at, updated_at, removed_at";
const ONBOARDING_DONE: &str = "onboarding_done";
const ONBOARDING_AREAS: &str = "onboarding_areas";

impl super::ChatStore {
    /// Contract one professional. The role already hired and active answers
    /// with that hire, unchanged, and says so in the outcome.
    pub fn hire(&self, new: NewHire) -> Result<HireOutcome, HireError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let outcome = insert_hire(&transaction, &new)?;
        transaction.commit()?;
        Ok(outcome)
    }

    /// Contract a whole team in one transaction: if one member fails, none is kept.
    pub fn hire_team(&self, roles: &[NewHire]) -> Result<Vec<Hire>, HireError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let hires = roles
            .iter()
            .map(|new| insert_hire(&transaction, new).map(HireOutcome::into_hire))
            .collect::<Result<Vec<_>, _>>()?;
        transaction.commit()?;
        Ok(hires)
    }

    pub fn list_hires(&self, filter: HireFilter) -> Result<Vec<Hire>, HireError> {
        let connection = self.lock();
        let sql = match filter {
            HireFilter::Active => hire_query("WHERE status = 'hired' ORDER BY created_at, id"),
            HireFilter::All => hire_query("ORDER BY created_at, id"),
        };
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], hire_row)?;
        let hires = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(hires)
    }

    pub fn get_hire(&self, hire_id: &str) -> Result<Hire, HireError> {
        let connection = self.lock();
        hire_by_id(&connection, hire_id)
    }

    /// Rename an active hire. The avatar is a character id, so it stays as it is.
    pub fn rename_hire(&self, hire_id: &str, name: &str) -> Result<Hire, HireError> {
        let name = agent_name(name)?;
        let connection = self.lock();
        ensure_active(&connection, hire_id)?;
        let now = schema::now(&connection)?;
        connection.execute(
            "UPDATE agent_hire SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now, hire_id],
        )?;
        hire_by_id(&connection, hire_id)
    }

    pub fn set_hire_avatar(&self, hire_id: &str, avatar: Option<&str>) -> Result<Hire, HireError> {
        let connection = self.lock();
        ensure_active(&connection, hire_id)?;
        let now = schema::now(&connection)?;
        connection.execute(
            "UPDATE agent_hire SET avatar = ?1, updated_at = ?2 WHERE id = ?3",
            params![avatar, now, hire_id],
        )?;
        hire_by_id(&connection, hire_id)
    }

    /// Dismiss a hire. Dismissing it again changes nothing, and the dismissed
    /// hire stops counting against its area's limit.
    pub fn dismiss_hire(&self, hire_id: &str) -> Result<Hire, HireError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let hire = hire_by_id(&transaction, hire_id)?;
        if hire.status == HireStatus::Removed {
            return Ok(hire);
        }
        let now = schema::now(&transaction)?;
        transaction.execute(
            "UPDATE agent_hire SET status = ?1, removed_at = ?2, updated_at = ?2 WHERE id = ?3",
            params![HireStatus::Removed.as_str(), now, hire_id],
        )?;
        // The seat goes with the hire: a removed seat holds no place in its project.
        transaction.execute(
            "UPDATE agent_seat SET status = 'removed' WHERE hire_id = ?1 AND status != 'removed'",
            [hire_id],
        )?;
        transaction.commit()?;
        hire_by_id(&connection, hire_id)
    }

    pub fn onboarding_state(&self) -> Result<OnboardingState, HireError> {
        let connection = self.lock();
        read_onboarding(&connection)
    }

    /// Mark the onboarding done. It only records the state: contracting the
    /// team of those areas is a separate step. Asking again changes nothing.
    pub fn complete_onboarding(&self, areas: &[String]) -> Result<OnboardingState, HireError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let current = read_onboarding(&transaction)?;
        if current.done {
            return Ok(current);
        }
        schema::set_meta(&transaction, ONBOARDING_DONE, "true")?;
        schema::set_meta(
            &transaction,
            ONBOARDING_AREAS,
            &serde_json::to_string(areas)?,
        )?;
        transaction.commit()?;
        Ok(OnboardingState {
            done: true,
            areas: areas.to_vec(),
        })
    }
}

fn insert_hire(connection: &Connection, new: &NewHire) -> Result<HireOutcome, HireError> {
    let name = agent_name(&new.name)?;
    if let Some(active) = active_hire_for_role(connection, &new.catalog_role)? {
        return Ok(HireOutcome::Existing(active));
    }
    let hired_in_area: i64 = connection.query_row(
        "SELECT COUNT(*) FROM agent_hire WHERE area = ?1 AND status = 'hired'",
        [&new.area],
        |row| row.get(0),
    )?;
    if hired_in_area as usize >= MAX_HIRES_PER_AREA {
        return Err(HireError::AreaFull(new.area.clone()));
    }
    let now = schema::now(connection)?;
    let hire_id = id::new_id("hire");
    let skills = serde_json::to_string(&new.skills_allow)?;
    connection.execute(
        "INSERT INTO agent_hire
            (id, company_id, catalog_role, area, name, avatar, engine, profile,
             skills_allow, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'hired', ?10, ?10)",
        params![
            hire_id,
            company_id(connection)?,
            new.catalog_role,
            new.area,
            name,
            new.avatar,
            new.engine,
            new.profile,
            skills,
            now,
        ],
    )?;
    Ok(HireOutcome::Created(hire_by_id(connection, &hire_id)?))
}

fn active_hire_for_role(
    connection: &Connection,
    catalog_role: &str,
) -> Result<Option<Hire>, HireError> {
    Ok(connection
        .query_row(
            &hire_query("WHERE catalog_role = ?1 AND status = 'hired'"),
            [catalog_role],
            hire_row,
        )
        .optional()?)
}

fn ensure_active(connection: &Connection, hire_id: &str) -> Result<(), HireError> {
    match hire_by_id(connection, hire_id)?.status {
        HireStatus::Hired => Ok(()),
        HireStatus::Removed => Err(HireError::HireNotActive(hire_id.to_string())),
    }
}

fn hire_by_id(connection: &Connection, hire_id: &str) -> Result<Hire, HireError> {
    connection
        .query_row(&hire_query("WHERE id = ?1"), [hire_id], hire_row)
        .optional()?
        .ok_or_else(|| HireError::NoSuchHire(hire_id.to_string()))
}

fn hire_query(clause: &str) -> String {
    format!("SELECT {HIRE_COLUMNS} FROM agent_hire {clause}")
}

/// Reads one row of `HIRE_COLUMNS`. A stored list or status that does not parse
/// is a conversion failure of its column, never a silent default.
fn hire_row(row: &Row<'_>) -> rusqlite::Result<Hire> {
    let skills: String = row.get(7)?;
    let status: String = row.get(8)?;
    Ok(Hire {
        id: row.get(0)?,
        catalog_role: row.get(1)?,
        area: row.get(2)?,
        name: row.get(3)?,
        avatar: row.get(4)?,
        engine: row.get(5)?,
        profile: row.get(6)?,
        skills_allow: serde_json::from_str(&skills)
            .map_err(|_| corrupt(7, "skills_allow is not a JSON list"))?,
        status: HireStatus::from_db(&status).ok_or_else(|| corrupt(8, "unknown hire status"))?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        removed_at: row.get(11)?,
    })
}

fn corrupt(column: usize, reason: &'static str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, Type::Text, reason.into())
}

fn read_onboarding(connection: &Connection) -> Result<OnboardingState, HireError> {
    let done = schema::meta_value(connection, ONBOARDING_DONE)?.as_deref() == Some("true");
    let areas = match schema::meta_value(connection, ONBOARDING_AREAS)? {
        Some(json) => serde_json::from_str(&json)?,
        None => Vec::new(),
    };
    Ok(OnboardingState { done, areas })
}
