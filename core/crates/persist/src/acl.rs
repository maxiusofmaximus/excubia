//! The data directory's own ACL. Windows gives a new folder the ACL it inherits
//! from its parent, and the profile grants more write rights than the host
//! wants: `chat.db`, the trust store (who holds ADMIN), the TLS key, the hook
//! token and the tools the host launches all sit in that folder. On Windows
//! `restrict_dir_to_owner` replaces the inherited list with three entries (the
//! current user, SYSTEM and the Administrators group), each with full control
//! and inherited by every child. Elsewhere the owner-only file modes already
//! protect the files, so the call does nothing.

/// Makes `dir` owner-only: a protected DACL with the current user, SYSTEM and
/// Administrators, inherited by everything inside. Returns `true` when it wrote
/// a new DACL and `false` when the folder already had exactly that one.
#[cfg(not(windows))]
pub fn restrict_dir_to_owner(_dir: &std::path::Path) -> std::io::Result<bool> {
    Ok(false)
}

#[cfg(windows)]
pub use windows_acl::restrict_dir_to_owner;

#[cfg(windows)]
mod windows_acl {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::path::Path;
    use std::ptr::{addr_of, null, null_mut};

    use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W,
        NO_MULTIPLE_TRUSTEE, SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
        TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_TYPE, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        CopySid, CreateWellKnownSid, GetAce, GetLengthSid, GetSecurityDescriptorControl,
        GetTokenInformation, TokenUser, WinBuiltinAdministratorsSid, WinLocalSystemSid,
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION,
        OBJECT_INHERIT_ACE, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        SECURITY_MAX_SID_SIZE, SE_DACL_PROTECTED, SUB_CONTAINERS_AND_OBJECTS_INHERIT, TOKEN_QUERY,
        TOKEN_USER, WELL_KNOWN_SID_TYPE,
    };
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// `FILE_ALL_ACCESS`: full control of a file or folder.
    const FULL_CONTROL: u32 = 0x001F_01FF;
    /// Both inheritance bits: the ACE reaches the folders and the files below.
    const INHERIT: u32 = CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE;

    /// Returns whether it wrote a new DACL. See the module docs.
    pub fn restrict_dir_to_owner(dir: &Path) -> io::Result<bool> {
        let owners = [
            current_user()?,
            well_known(WinLocalSystemSid)?,
            well_known(WinBuiltinAdministratorsSid)?,
        ];
        let path: Vec<u16> = dir
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        if is_restricted(&path, &owners)? {
            return Ok(false);
        }
        let [user, system, admins] = &owners;
        let entries = [
            owner_entry(user, TRUSTEE_IS_USER),
            owner_entry(system, TRUSTEE_IS_WELL_KNOWN_GROUP),
            owner_entry(admins, TRUSTEE_IS_WELL_KNOWN_GROUP),
        ];
        let mut acl: *mut ACL = null_mut();
        // SAFETY: `entries` lives across the call and its SID pointers borrow `owners`,
        // which lives as long as this function. SetEntriesInAclW allocates `acl`,
        // which LocalMem frees.
        let code = unsafe { SetEntriesInAclW(3, entries.as_ptr(), null(), &mut acl) };
        if code != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let acl = LocalMem(acl);
        // SAFETY: `path` is NUL-terminated and lives across the call. The owner and
        // group are left as they are (null), and `acl` is the DACL built above.
        let code = unsafe {
            SetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl.0,
                null(),
            )
        };
        if code != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        Ok(true)
    }

    /// True when the DACL is protected and holds exactly one full-control ACE
    /// per owner, inheritable to children. Anything else gets rewritten.
    fn is_restricted(path: &[u16], owners: &[Vec<u32>]) -> io::Result<bool> {
        let dacl = read_dacl(path)?;
        Ok(dacl.protected
            && dacl.aces.len() == owners.len()
            && owners
                .iter()
                .all(|owner| dacl.aces.iter().any(|ace| ace.sid == *owner))
            && dacl.aces.iter().all(|ace| {
                ace.kind == ACCESS_ALLOWED_ACE_TYPE as u8
                    && ace.mask == FULL_CONTROL
                    && ace.flags & INHERIT == INHERIT
            }))
    }

    /// One ACE of a DACL. Only access-allowed ACEs carry a SID and a mask; any
    /// other kind is kept with an empty SID so it never matches an owner.
    struct Ace {
        kind: u8,
        flags: u32,
        mask: u32,
        sid: Vec<u32>,
    }

    struct Dacl {
        protected: bool,
        aces: Vec<Ace>,
    }

    /// Reads the DACL of `path` and whether it is protected from inheritance.
    fn read_dacl(path: &[u16]) -> io::Result<Dacl> {
        let mut dacl: *mut ACL = null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: `path` is NUL-terminated and lives across the call. On success
        // `descriptor` owns `dacl`, and LocalMem frees it once.
        let code = unsafe {
            GetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut descriptor,
            )
        };
        if code != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let descriptor = LocalMem(descriptor);
        let mut control: u16 = 0;
        let mut revision: u32 = 0;
        // SAFETY: `descriptor` is the valid descriptor returned above and still alive.
        if unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let protected = control & SE_DACL_PROTECTED != 0;
        if dacl.is_null() {
            return Ok(Dacl {
                protected,
                aces: Vec::new(),
            });
        }
        // SAFETY: `dacl` is non-null and points into the descriptor, which is alive.
        let count = unsafe { (*dacl).AceCount };
        let mut aces = Vec::with_capacity(usize::from(count));
        for index in 0..u32::from(count) {
            let mut ace: *mut c_void = null_mut();
            // SAFETY: `index` is below AceCount, so GetAce returns an ACE inside `dacl`.
            if unsafe { GetAce(dacl, index, &mut ace) } == 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: every ACE begins with an ACE_HEADER.
            let header = unsafe { *ace.cast::<ACE_HEADER>() };
            let kind = header.AceType;
            let flags = u32::from(header.AceFlags);
            if u32::from(kind) != ACCESS_ALLOWED_ACE_TYPE {
                aces.push(Ace {
                    kind,
                    flags,
                    mask: 0,
                    sid: Vec::new(),
                });
                continue;
            }
            // SAFETY: an access-allowed ACE has the ACCESS_ALLOWED_ACE layout, and its
            // SID starts at `SidStart` inside the ACE.
            let (mask, sid) = unsafe {
                let allowed = ace.cast::<ACCESS_ALLOWED_ACE>();
                (
                    (*allowed).Mask,
                    addr_of!((*allowed).SidStart).cast_mut().cast::<c_void>(),
                )
            };
            aces.push(Ace {
                kind,
                flags,
                mask,
                sid: copy_sid(sid)?,
            });
        }
        Ok(Dacl { protected, aces })
    }

    fn owner_entry(sid: &[u32], kind: TRUSTEE_TYPE) -> EXPLICIT_ACCESS_W {
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FULL_CONTROL,
            grfAccessMode: SET_ACCESS,
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: kind,
                ptstrName: sid.as_ptr().cast_mut().cast(),
            },
        }
    }

    /// The SID of the account the process runs as, from its token.
    fn current_user() -> io::Result<Vec<u32>> {
        let mut token: HANDLE = null_mut();
        // SAFETY: GetCurrentProcess is a pseudo-handle that needs no closing; `token`
        // receives a real handle, which OwnedHandle below closes once.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: OpenProcessToken succeeded, so `token` is a handle we own.
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        let mut needed: u32 = 0;
        // SAFETY: a null buffer of length 0 only asks for the size; the call fails and sets `needed`.
        unsafe {
            GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut needed)
        };
        // A u64 buffer keeps the TOKEN_USER 8-byte aligned.
        let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
        // SAFETY: `buffer` holds `needed` bytes and is aligned for a TOKEN_USER.
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the call succeeded, so `buffer` starts with a TOKEN_USER whose SID lies inside it.
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        copy_sid(sid)
    }

    /// A well-known SID such as SYSTEM or Administrators.
    fn well_known(kind: WELL_KNOWN_SID_TYPE) -> io::Result<Vec<u32>> {
        let mut buffer = vec![0u32; (SECURITY_MAX_SID_SIZE as usize).div_ceil(4)];
        let mut size = SECURITY_MAX_SID_SIZE;
        // SAFETY: `buffer` has SECURITY_MAX_SID_SIZE bytes, the largest possible SID.
        if unsafe { CreateWellKnownSid(kind, null_mut(), buffer.as_mut_ptr().cast(), &mut size) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        copy_sid(buffer.as_mut_ptr().cast())
    }

    /// Copies a SID into 4-byte words, so that equal SIDs compare equal as vectors.
    fn copy_sid(sid: PSID) -> io::Result<Vec<u32>> {
        // SAFETY: `sid` points to a valid SID, as the callers above guarantee.
        let length = unsafe { GetLengthSid(sid) } as usize;
        let mut copy = vec![0u32; length.div_ceil(4)];
        // SAFETY: `copy` holds at least `length` bytes, and CopySid writes exactly that many.
        if unsafe { CopySid(length as u32, copy.as_mut_ptr().cast(), sid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(copy)
    }

    /// Frees memory that a Windows security call allocated with LocalAlloc.
    struct LocalMem<T>(*mut T);

    impl<T> Drop for LocalMem<T> {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the pointer came from a Windows call that allocates with
                // LocalAlloc, and it is freed exactly once, here.
                unsafe { LocalFree(self.0.cast()) };
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use excubia_test_dir::TestDir;
        use std::fs;

        /// Set on an ACE that a folder handed down to a child (`INHERITED_ACE`).
        const INHERITED: u32 = 0x10;

        /// The three SIDs a restricted folder must hold, in no particular order.
        fn expected_owners() -> Vec<Vec<u32>> {
            vec![
                current_user().expect("current user"),
                well_known(WinLocalSystemSid).expect("SYSTEM"),
                well_known(WinBuiltinAdministratorsSid).expect("Administrators"),
            ]
        }

        fn wide(path: &Path) -> Vec<u16> {
            path.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        }

        #[test]
        fn the_folder_keeps_three_owners_and_its_children_inherit_them() {
            let dir = TestDir::new("acl-owners");
            let before = dir.path().join("before.txt");
            fs::write(&before, b"before").expect("write before");
            let nested = dir.path().join("sub");
            fs::create_dir(&nested).expect("create sub");
            let deep = nested.join("deep.txt");
            fs::write(&deep, b"deep").expect("write deep");
            let owners = expected_owners();

            assert!(
                restrict_dir_to_owner(dir.path()).expect("first call"),
                "the first call writes"
            );

            // (a) the user can still read and write, old files and new ones.
            assert_eq!(fs::read(&before).expect("read before"), b"before");
            fs::write(dir.path().join("after.txt"), b"after").expect("write after");
            assert_eq!(
                fs::read(dir.path().join("after.txt")).expect("read after"),
                b"after"
            );

            // (b) the folder's DACL is protected and holds exactly the three owners.
            let folder = read_dacl(&wide(dir.path())).expect("read folder DACL");
            assert!(
                folder.protected,
                "the DACL does not inherit from the parent"
            );
            assert_eq!(folder.aces.len(), 3);
            for ace in &folder.aces {
                assert_eq!(ace.kind, ACCESS_ALLOWED_ACE_TYPE as u8);
                assert_eq!(ace.mask, FULL_CONTROL);
                assert_eq!(ace.flags & INHERIT, INHERIT, "children inherit it");
            }
            for owner in &owners {
                assert!(
                    folder.aces.iter().any(|ace| ace.sid == *owner),
                    "an owner is missing"
                );
            }

            // (c) a second call finds the DACL it wants and writes nothing.
            assert!(!restrict_dir_to_owner(dir.path()).expect("second call"));

            // (d) the files that existed before, one level down and two, inherit the same three.
            for file in [&before, &deep] {
                let dacl = read_dacl(&wide(file)).expect("read file DACL");
                let inherited: Vec<&Ace> = dacl
                    .aces
                    .iter()
                    .filter(|ace| ace.flags & INHERITED != 0)
                    .collect();
                assert_eq!(inherited.len(), 3, "the old inherited ACEs are replaced");
                for owner in &owners {
                    assert!(inherited.iter().any(|ace| ace.sid == *owner));
                }
            }
        }
    }
}
