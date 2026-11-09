use crate::App;
use thiserror::Error;

/// Every way a document operation can fail. The messages are shown to the user
/// in the chat, so they carry no paths.
#[derive(Debug, Error)]
pub enum CraftError {
    #[error("Falta {} en este PC. El Master puede instalarla desde Herramientas de documentos", .0.name())]
    ToolMissing(App),
    #[error("No se puede abrir: {0}")]
    Unsupported(&'static str),
    #[error("La página {0} no existe en el documento")]
    PageOutOfRange(u32),
    #[error("El documento no es válido: {0}")]
    BadDocument(&'static str),
    #[error("Opción no válida: {0}")]
    Invalid(&'static str),
    #[error("{} tardó demasiado y se detuvo", .0.name())]
    Timeout(App),
    #[error("{} falló: {}", .0.name(), .1)]
    Failed(App, String),
    #[error("La vista previa no es válida: {0}")]
    BadOutput(&'static str),
    #[error("No se pudo arrancar {}", .0.name())]
    Spawn(App, #[source] std::io::Error),
    #[error("crafting.json no es válido")]
    BadConfig,
    #[error("No se pudo usar la carpeta de trabajo: {0}")]
    Io(#[from] std::io::Error),
}
