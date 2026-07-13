//! `storage` — persistencia local en SQLite y los repositorios que aplican
//! aislamiento por alcance (D3) y auditoría (D11) en un punto único de acceso.
//!
//! El backend concreto es [`Almacen`]; las operaciones se exponen vía las traits
//! de [`repos`] (agnósticas de almacenamiento). El motor de sincronización hacia
//! Supabase es un cambio futuro; el esquema ya nace sync-ready (D12).

mod almacen;
pub mod error;
mod migraciones;
pub mod repos;

pub use almacen::Almacen;
pub use error::{ErrorAlmacen, Resultado};

/// Trae las traits de repositorio al ámbito para operar sobre un [`Almacen`].
pub mod prelude {
    pub use crate::Almacen;
    pub use crate::repos::{
        AltaCodigo, Auditoria, BorradorProducto, Caja, Catalogo, EntradaBitacora, Inventario,
        Organizacion, Precios,
    };
}
