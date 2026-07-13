//! Código de barras EAN-13: valida GTIN externos y **genera** el barcode estable
//! de los productos que produce la matriz (D22).
//!
//! El generado usa un **prefijo interno** (rango GS1 de circulación restringida
//! `20`–`29`) que no colisiona con GTIN comerciales externos. Es **uno por
//! producto (SKU)**: todas las unidades comparten el mismo código, impreso en el
//! empaque como un producto comercial — no es un código por-ítem.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::ErrorDominio;

/// Prefijo interno por defecto para los barcodes acuñados por matriz (`2`
/// reservado por GS1 a circulación restringida in-store).
pub const PREFIJO_MATRIZ: &str = "200";

/// Un EAN-13 válido (13 dígitos, dígito verificador correcto).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ean13(String);

impl Ean13 {
    /// Valida una cadena como EAN-13 (longitud, dígitos y verificador).
    pub fn parse(codigo: &str) -> Result<Self, ErrorDominio> {
        if codigo.len() != 13 || !codigo.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ErrorDominio::Invalido(format!(
                "EAN-13 debe ser 13 dígitos: {codigo}"
            )));
        }
        let digitos = Self::digitos(codigo);
        let esperado = Self::verificador(&digitos[..12]);
        if digitos[12] != esperado {
            return Err(ErrorDominio::Invalido(format!(
                "dígito verificador inválido en {codigo}"
            )));
        }
        Ok(Ean13(codigo.to_string()))
    }

    /// Genera el EAN-13 de un producto de matriz a partir de una **secuencia
    /// interna** única. El prefijo separa el espacio de códigos internos de los
    /// GTIN externos; la secuencia garantiza unicidad entre productos.
    pub fn generar_matriz(prefijo: &str, secuencia: u64) -> Result<Self, ErrorDominio> {
        if prefijo.is_empty() || prefijo.len() >= 12 || !prefijo.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(ErrorDominio::Invalido(format!(
                "prefijo interno inválido: {prefijo}"
            )));
        }
        let cuerpo_len = 12 - prefijo.len();
        let max = 10u64.pow(cuerpo_len as u32);
        if secuencia >= max {
            return Err(ErrorDominio::Invalido(format!(
                "secuencia {secuencia} excede el espacio del prefijo {prefijo}"
            )));
        }
        let base = format!("{prefijo}{secuencia:0cuerpo_len$}");
        let digitos = Self::digitos(&base);
        let verificador = Self::verificador(&digitos);
        Ok(Ean13(format!("{base}{verificador}")))
    }

    #[must_use]
    pub fn get(&self) -> &str {
        &self.0
    }

    fn digitos(s: &str) -> Vec<u8> {
        s.bytes().map(|b| b - b'0').collect()
    }

    /// Dígito verificador EAN-13 sobre los 12 primeros dígitos: posiciones
    /// impares (1-indexadas desde la izquierda) pesan 1, las pares pesan 3.
    fn verificador(primeros12: &[u8]) -> u8 {
        let suma: u32 = primeros12
            .iter()
            .enumerate()
            .map(|(i, &d)| u32::from(d) * if i % 2 == 0 { 1 } else { 3 })
            .sum();
        ((10 - (suma % 10)) % 10) as u8
    }
}

impl fmt::Display for Ean13 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valida_gtin_externo_real() {
        // GTIN comercial con verificador correcto.
        assert!(Ean13::parse("7501059224827").is_ok());
    }

    #[test]
    fn rechaza_verificador_incorrecto() {
        assert!(Ean13::parse("7501059224820").is_err());
    }

    #[test]
    fn rechaza_longitud_y_no_digitos() {
        assert!(Ean13::parse("12345").is_err());
        assert!(Ean13::parse("75010592248AB").is_err());
    }

    #[test]
    fn genera_matriz_decodable_y_con_prefijo() {
        let a = Ean13::generar_matriz(PREFIJO_MATRIZ, 1).unwrap();
        assert_eq!(a.get().len(), 13);
        assert!(a.get().starts_with(PREFIJO_MATRIZ));
        // el generado se re-valida como EAN-13 legítimo (verificador correcto)
        assert!(Ean13::parse(a.get()).is_ok());
    }

    #[test]
    fn secuencias_distintas_dan_codigos_distintos() {
        let a = Ean13::generar_matriz(PREFIJO_MATRIZ, 1).unwrap();
        let b = Ean13::generar_matriz(PREFIJO_MATRIZ, 2).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn secuencia_fuera_de_rango_se_rechaza() {
        // prefijo "200" deja 9 dígitos de cuerpo → 10^9 combinaciones
        assert!(Ean13::generar_matriz("200", 1_000_000_000).is_err());
    }
}
