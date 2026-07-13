//! Tiempo: instante en **UTC capturado en el nodo**; la zona local solo sirve
//! para presentar y para definir "el día" de reportes (D21).
//!
//! El servidor central nunca reescribe la marca del nodo; un offset fijo
//! hardcodeado se rompe con husos y cambios de horario, así que se usa el
//! identificador **IANA** de zona (p. ej. `America/Mexico_City`).

use std::fmt;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use serde::{Deserialize, Serialize};

use crate::error::ErrorDominio;

/// Zona horaria por defecto de una sucursal.
pub const ZONA_POR_DEFECTO: &str = "America/Mexico_City";

/// Instante en UTC. Se captura en el nodo donde ocurre la operación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Instante(Timestamp);

impl Instante {
    /// Captura el instante actual **en el nodo** (único punto de lectura del reloj).
    #[must_use]
    pub fn ahora() -> Self {
        Instante(Timestamp::now())
    }

    #[must_use]
    pub fn timestamp(self) -> Timestamp {
        self.0
    }

    #[must_use]
    pub fn desde_timestamp(ts: Timestamp) -> Self {
        Instante(ts)
    }

    /// Segundos enteros transcurridos desde la época UNIX (para medir lapsos).
    #[must_use]
    pub fn segundos_epoch(self) -> i64 {
        self.0.as_second()
    }

    /// Presenta el instante en la hora local de una sucursal.
    #[must_use]
    pub fn en_zona(self, zona: &ZonaHoraria) -> Zoned {
        self.0.to_zoned(zona.tz())
    }

    /// El **día natural local** de la sucursal (frontera de reportes y cortes).
    #[must_use]
    pub fn dia_local(self, zona: &ZonaHoraria) -> Date {
        self.0.to_zoned(zona.tz()).date()
    }
}

impl fmt::Display for Instante {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Zona horaria de una sucursal (identificador IANA validado).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ZonaHoraria(String);

impl ZonaHoraria {
    /// Valida el identificador IANA contra la base de zonas embebida.
    pub fn nueva(iana: &str) -> Result<Self, ErrorDominio> {
        TimeZone::get(iana).map_err(|_| {
            ErrorDominio::Invalido(format!("zona horaria IANA desconocida: {iana}"))
        })?;
        Ok(ZonaHoraria(iana.to_string()))
    }

    #[must_use]
    pub fn iana(&self) -> &str {
        &self.0
    }

    fn tz(&self) -> TimeZone {
        // Ya se validó en `nueva`; si falla aquí, se cae a UTC en vez de entrar en pánico.
        TimeZone::get(&self.0).unwrap_or(TimeZone::UTC)
    }
}

impl Default for ZonaHoraria {
    fn default() -> Self {
        ZonaHoraria(ZONA_POR_DEFECTO.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zona_valida_y_default() {
        assert!(ZonaHoraria::nueva("America/Mexico_City").is_ok());
        assert_eq!(ZonaHoraria::default().iana(), ZONA_POR_DEFECTO);
    }

    #[test]
    fn zona_invalida_se_rechaza() {
        assert!(ZonaHoraria::nueva("Marte/Olympus").is_err());
    }

    #[test]
    fn dia_local_usa_la_zona_de_la_sucursal() {
        // 2026-01-01T05:30:00Z es aún 2025-12-31 en horario de la Ciudad de México (UTC-6).
        let ts: Timestamp = "2026-01-01T05:30:00Z".parse().unwrap();
        let inst = Instante::desde_timestamp(ts);
        let zona = ZonaHoraria::nueva("America/Mexico_City").unwrap();
        assert_eq!(inst.dia_local(&zona).to_string(), "2025-12-31");
    }
}
