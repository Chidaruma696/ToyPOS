//! Errores del dominio.

use crate::acceso::Permiso;

/// Falla de una regla de negocio o de una comprobación de acceso.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErrorDominio {
    /// El actor no porta el permiso requerido.
    #[error("permiso denegado: falta `{0}`")]
    PermisoDenegado(Permiso),

    /// El alcance del actor no cubre la sucursal objetivo (aislamiento, D3).
    #[error("fuera de alcance: la operación toca una sucursal que no está en tu alcance")]
    FueraDeAlcance,

    /// La operación exige un usuario en sesión (no el actor `sistema`).
    #[error("se requiere un usuario en sesión para esta operación")]
    RequiereUsuario,

    /// Transición de estado no permitida (p. ej. re-resolver una aprobación terminal).
    #[error("transición inválida: {0}")]
    TransicionInvalida(String),

    /// Segregación de deberes: el solicitante no puede resolver su propia solicitud.
    #[error("segregación de deberes: no puedes resolver tu propia solicitud")]
    SegregacionDeDeberes,

    /// Dato de entrada inválido (formato, rango, etc.).
    #[error("dato inválido: {0}")]
    Invalido(String),

    /// Regla de negocio violada.
    #[error("regla violada: {0}")]
    Regla(String),
}
