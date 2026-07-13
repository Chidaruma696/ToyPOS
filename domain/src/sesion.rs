//! Sesión de trabajo: envuelve el `ContextoAcceso` con la **sucursal activa**
//! dentro del alcance (D3/D16) y la regla de **cierre por inactividad** (D16).
//!
//! Ambas reglas viven aquí (DRY); la cara futura solo las invoca. La selección
//! de sucursal activa **nunca amplía** el alcance: solo filtra dentro de él.

use uuid::Uuid;

use crate::acceso::{Alcance, ContextoAcceso};
use crate::error::ErrorDominio;
use crate::tiempo::Instante;

/// Sucursal activa que corresponde automáticamente a un alcance: si es una sola
/// sucursal, esa; si son varias o todas, ninguna por defecto (la cara elige).
#[must_use]
pub fn activa_automatica(alcance: &Alcance) -> Option<Uuid> {
    match alcance {
        Alcance::Sucursales(s) if s.len() == 1 => s.iter().next().copied(),
        _ => None,
    }
}

/// Sesión viva de un usuario autenticado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sesion {
    pub ctx: ContextoAcceso,
    pub sucursal_activa: Option<Uuid>,
    pub ultima_actividad: Instante,
}

impl Sesion {
    /// Inicia la sesión tras el login; la sucursal activa se resuelve del alcance.
    #[must_use]
    pub fn iniciar(ctx: ContextoAcceso, ahora: Instante) -> Self {
        let sucursal_activa = activa_automatica(&ctx.alcance);
        Sesion {
            ctx,
            sucursal_activa,
            ultima_actividad: ahora,
        }
    }

    /// Marca actividad reciente (reinicia el reloj de inactividad).
    pub fn tocar(&mut self, ahora: Instante) {
        self.ultima_actividad = ahora;
    }

    /// ¿La sesión rebasó el periodo de inactividad? De ser así, la cara hace un
    /// **logout completo** (no hay desbloqueo por PIN, D16).
    #[must_use]
    pub fn expirada(&self, ahora: Instante, max_inactividad_segundos: i64) -> bool {
        ahora.segundos_epoch() - self.ultima_actividad.segundos_epoch() > max_inactividad_segundos
    }

    /// Selecciona la sucursal activa **dentro del alcance**. Rechaza cualquier
    /// sucursal fuera de él: la selección nunca amplía el alcance (D3).
    pub fn seleccionar_sucursal(&mut self, sucursal: Uuid) -> Result<(), ErrorDominio> {
        if !self.ctx.alcance.cubre(sucursal) {
            return Err(ErrorDominio::FueraDeAlcance);
        }
        self.sucursal_activa = Some(sucursal);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceso::Actor;
    use std::collections::BTreeSet;

    fn ctx(alcance: Alcance) -> ContextoAcceso {
        ContextoAcceso::nuevo(Actor::Usuario(Uuid::new_v4()), BTreeSet::new(), alcance)
    }

    #[test]
    fn alcance_de_una_activa_automatica() {
        let a = Uuid::new_v4();
        let s = Sesion::iniciar(ctx(Alcance::de([a])), Instante::ahora());
        assert_eq!(s.sucursal_activa, Some(a));
    }

    #[test]
    fn alcance_multiple_no_elige_por_defecto() {
        let s = Sesion::iniciar(
            ctx(Alcance::de([Uuid::new_v4(), Uuid::new_v4()])),
            Instante::ahora(),
        );
        assert_eq!(s.sucursal_activa, None);
    }

    #[test]
    fn seleccionar_fuera_de_alcance_se_rechaza() {
        let a = Uuid::new_v4();
        let mut s = Sesion::iniciar(ctx(Alcance::de([a])), Instante::ahora());
        assert_eq!(
            s.seleccionar_sucursal(Uuid::new_v4()),
            Err(ErrorDominio::FueraDeAlcance)
        );
        assert!(s.seleccionar_sucursal(a).is_ok());
    }

    #[test]
    fn expira_por_inactividad() {
        let t0 = Instante::desde_timestamp("2026-01-01T10:00:00Z".parse().unwrap());
        let s = Sesion::iniciar(ctx(Alcance::Todas), t0);
        let t_poco = Instante::desde_timestamp("2026-01-01T10:04:00Z".parse().unwrap());
        let t_mucho = Instante::desde_timestamp("2026-01-01T10:06:00Z".parse().unwrap());
        assert!(!s.expirada(t_poco, 300)); // 4 min < 5 min
        assert!(s.expirada(t_mucho, 300)); // 6 min > 5 min
    }
}
