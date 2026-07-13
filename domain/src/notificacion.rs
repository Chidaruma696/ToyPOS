//! Notificaciones del **sistema a usuarios** (D37): bandeja mínima por
//! sucursal, no bloqueante, con marcar-leída a nivel sucursal. Es un mecanismo
//! distinto del subsistema de autorizaciones (que resuelve solicitudes, D4).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::acceso::{Actor, ContextoAcceso};
use crate::error::ErrorDominio;
use crate::tiempo::Instante;

/// Tipo de notificación. Extensible por variante (como los permisos, D2):
/// `PorVencer` la emite el barrido de caducidad (D36) y `DiscrepanciaEnvio` la
/// recepción con faltantes (D41).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoNotificacion {
    PorVencer,
    DiscrepanciaEnvio,
}

/// Un aviso operativo dirigido a la bandeja de una sucursal. Se marca leída
/// **por sucursal** (es del local, no correo personal) y nunca se borra (D37).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notificacion {
    pub id: Uuid,
    pub tipo: TipoNotificacion,
    pub mensaje: String,
    pub sucursal: Uuid,
    pub creado: Instante,
    pub leida_en: Option<Instante>,
    pub actualizado: Instante,
}

impl Notificacion {
    /// Emite una notificación: **solo el sistema** (D37); el mensaje no puede
    /// estar vacío. Nace no leída.
    pub fn emitir(
        ctx: &ContextoAcceso,
        tipo: TipoNotificacion,
        mensaje: &str,
        sucursal: Uuid,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        if ctx.actor != Actor::Sistema {
            return Err(ErrorDominio::Regla(
                "solo el sistema emite notificaciones".into(),
            ));
        }
        let mensaje = mensaje.trim();
        if mensaje.is_empty() {
            return Err(ErrorDominio::Invalido(
                "el mensaje de la notificación es obligatorio".into(),
            ));
        }
        Ok(Notificacion {
            id: Uuid::new_v4(),
            tipo,
            mensaje: mensaje.to_string(),
            sucursal,
            creado: ahora,
            leida_en: None,
            actualizado: ahora,
        })
    }

    /// Marca la notificación como leída. **Idempotente**: una ya leída conserva
    /// su marca de tiempo original y no falla.
    pub fn marcar_leida(&mut self, ahora: Instante) {
        if self.leida_en.is_none() {
            self.leida_en = Some(ahora);
            self.actualizado = ahora;
        }
    }

    #[must_use]
    pub fn leida(&self) -> bool {
        self.leida_en.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceso::{Alcance, ContextoAcceso};
    use std::collections::BTreeSet;

    #[test]
    fn solo_el_sistema_emite() {
        let usuario = ContextoAcceso::nuevo(
            Actor::Usuario(Uuid::new_v4()),
            BTreeSet::new(),
            Alcance::Todas,
        );
        assert!(
            Notificacion::emitir(
                &usuario,
                TipoNotificacion::PorVencer,
                "x",
                Uuid::new_v4(),
                Instante::ahora()
            )
            .is_err()
        );
        assert!(
            Notificacion::emitir(
                &ContextoAcceso::sistema(),
                TipoNotificacion::PorVencer,
                "40 pzas por vencer",
                Uuid::new_v4(),
                Instante::ahora()
            )
            .is_ok()
        );
    }

    #[test]
    fn mensaje_vacio_se_rechaza() {
        let r = Notificacion::emitir(
            &ContextoAcceso::sistema(),
            TipoNotificacion::PorVencer,
            "   ",
            Uuid::new_v4(),
            Instante::ahora(),
        );
        assert!(r.is_err());
    }

    #[test]
    fn marcar_leida_es_idempotente_y_conserva_la_marca() {
        let mut n = Notificacion::emitir(
            &ContextoAcceso::sistema(),
            TipoNotificacion::PorVencer,
            "aviso",
            Uuid::new_v4(),
            Instante::ahora(),
        )
        .unwrap();
        assert!(!n.leida());
        let t1 = Instante::ahora();
        n.marcar_leida(t1);
        assert_eq!(n.leida_en, Some(t1));
        n.marcar_leida(Instante::ahora()); // segunda vez: sin cambio
        assert_eq!(n.leida_en, Some(t1));
    }
}
