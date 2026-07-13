//! Errores de la capa de almacenamiento.

use domain::ErrorDominio;

#[derive(Debug, thiserror::Error)]
pub enum ErrorAlmacen {
    /// Una regla o comprobación de acceso del dominio falló.
    #[error(transparent)]
    Dominio(#[from] ErrorDominio),

    /// Falla del motor SQLite.
    #[error("error de base de datos: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// Violación de un invariante de la colección (unicidad, matriz única, etc.).
    #[error("conflicto: {0}")]
    Conflicto(String),

    /// La entidad referida no existe (o quedó fuera del alcance).
    #[error("no encontrado: {0}")]
    NoEncontrado(String),

    /// Datos corruptos o inconsistentes al leer de la base.
    #[error("dato corrupto en la base: {0}")]
    Corrupto(String),
}

pub type Resultado<T> = Result<T, ErrorAlmacen>;
