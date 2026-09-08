//! Sucursales: `matriz` (única) o `expendio`, con **código** corto inmutable que
//! prefija sus folios, y **zona horaria** IANA (D18/D21).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ErrorDominio;
use crate::tiempo::{Instante, ZonaHoraria};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoSucursal {
    Matriz,
    Expendio,
}

/// Código de sucursal: 2–4 alfanuméricos en mayúsculas, **único e inmutable**,
/// asignado al alta (no derivado del nombre). Prefija los folios (D18).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CodigoSucursal(String);

impl CodigoSucursal {
    pub fn nueva(codigo: &str) -> Result<Self, ErrorDominio> {
        let c = codigo.trim().to_uppercase();
        let ok = (2..=4).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_alphanumeric());
        if !ok {
            return Err(ErrorDominio::Invalido(format!(
                "código de sucursal inválido (2–4 alfanuméricos): {codigo}"
            )));
        }
        Ok(CodigoSucursal(c))
    }

    #[must_use]
    pub fn get(&self) -> &str {
        &self.0
    }
}

/// Una sucursal. La unicidad de la matriz y del código son invariantes de la
/// colección: las verifica la capa de almacenamiento contra el resto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sucursal {
    pub id: Uuid,
    pub tipo: TipoSucursal,
    pub codigo: CodigoSucursal,
    pub nombre: String,
    pub zona: ZonaHoraria,
    pub activo: bool,
    pub actualizado: Instante,
}

impl Sucursal {
    pub fn nueva(
        tipo: TipoSucursal,
        codigo: CodigoSucursal,
        nombre: &str,
        zona: ZonaHoraria,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        if nombre.trim().is_empty() {
            return Err(ErrorDominio::Invalido(
                "el nombre de la sucursal es obligatorio".into(),
            ));
        }
        Ok(Sucursal {
            id: Uuid::new_v4(),
            tipo,
            codigo,
            nombre: nombre.trim().to_string(),
            zona,
            activo: true,
            actualizado: ahora,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codigo_normaliza_a_mayusculas() {
        assert_eq!(CodigoSucursal::nueva("snj").unwrap().get(), "SRO");
    }

    #[test]
    fn codigo_rechaza_longitud_y_simbolos() {
        assert!(CodigoSucursal::nueva("S").is_err());
        assert!(CodigoSucursal::nueva("SANJUAN").is_err());
        assert!(CodigoSucursal::nueva("S-J").is_err());
    }

    #[test]
    fn codigos_distintos_para_nombres_parecidos() {
        // Santa Rosa y Santa Rita reciben códigos explícitos que no colisionan.
        assert_ne!(
            CodigoSucursal::nueva("SRO").unwrap(),
            CodigoSucursal::nueva("SRI").unwrap()
        );
    }
}
