//! Aprobación **concreta de gastos** con máquina de estados reutilizable (D4).
//!
//! `pendiente → aprobada | rechazada | cancelada`; los tres finales son
//! terminales e inmutables. Autorizar es autoridad administrativa: exige
//! `AutorizarGasto` + alcance sobre la sucursal del gasto (el cajero no lo
//! porta). El solicitante no resuelve lo suyo salvo `AutoaprobarGasto`; cancelar
//! lo hace un aprobador, nunca el solicitante.

use uuid::Uuid;

use crate::acceso::{ContextoAcceso, Permiso};
use crate::error::ErrorDominio;
use crate::tiempo::Instante;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoAprobacion {
    Pendiente,
    Aprobada,
    Rechazada,
    Cancelada,
}

impl EstadoAprobacion {
    #[must_use]
    pub fn es_terminal(self) -> bool {
        !matches!(self, EstadoAprobacion::Pendiente)
    }
}

/// Solicitud de aprobación ligada a un gasto y al alcance de su sucursal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aprobacion {
    pub id: Uuid,
    pub gasto: Uuid,
    pub sucursal: Uuid,
    pub solicitante: Uuid,
    pub estado: EstadoAprobacion,
    pub resolutor: Option<Uuid>,
    pub resuelto_en: Option<Instante>,
    pub motivo: Option<String>,
    pub actualizado: Instante,
}

impl Aprobacion {
    /// Nace `pendiente` al registrarse el gasto.
    #[must_use]
    pub fn pendiente(gasto: Uuid, sucursal: Uuid, solicitante: Uuid, ahora: Instante) -> Self {
        Aprobacion {
            id: Uuid::new_v4(),
            gasto,
            sucursal,
            solicitante,
            estado: EstadoAprobacion::Pendiente,
            resolutor: None,
            resuelto_en: None,
            motivo: None,
            actualizado: ahora,
        }
    }

    /// Aprueba: el gasto pasa a autorizado (restará en su corte).
    pub fn aprobar(&mut self, ctx: &ContextoAcceso, ahora: Instante) -> Result<(), ErrorDominio> {
        let resolutor = self.autoridad_para_resolver(ctx)?;
        self.exigir_no_terminal()?;
        self.estado = EstadoAprobacion::Aprobada;
        self.sellar(resolutor, None, ahora);
        Ok(())
    }

    /// Rechaza con motivo obligatorio.
    pub fn rechazar(
        &mut self,
        ctx: &ContextoAcceso,
        motivo: &str,
        ahora: Instante,
    ) -> Result<(), ErrorDominio> {
        let resolutor = self.autoridad_para_resolver(ctx)?;
        self.exigir_no_terminal()?;
        let motivo = Self::motivo_valido(motivo)?;
        self.estado = EstadoAprobacion::Rechazada;
        self.sellar(resolutor, Some(motivo), ahora);
        Ok(())
    }

    /// Cancela (gasto por error) con motivo. La resuelve un aprobador, **nunca**
    /// el solicitante (sin excepción de auto-aprobación).
    pub fn cancelar(
        &mut self,
        ctx: &ContextoAcceso,
        motivo: &str,
        ahora: Instante,
    ) -> Result<(), ErrorDominio> {
        ctx.requiere_en(Permiso::AutorizarGasto, self.sucursal)?;
        let resolutor = ctx.usuario()?;
        if resolutor == self.solicitante {
            return Err(ErrorDominio::SegregacionDeDeberes);
        }
        self.exigir_no_terminal()?;
        let motivo = Self::motivo_valido(motivo)?;
        self.estado = EstadoAprobacion::Cancelada;
        self.sellar(resolutor, Some(motivo), ahora);
        Ok(())
    }

    #[must_use]
    pub fn autorizado(&self) -> bool {
        self.estado == EstadoAprobacion::Aprobada
    }

    /// Comprueba permiso + alcance + segregación de deberes para aprobar/rechazar.
    fn autoridad_para_resolver(&self, ctx: &ContextoAcceso) -> Result<Uuid, ErrorDominio> {
        ctx.requiere_en(Permiso::AutorizarGasto, self.sucursal)?;
        let resolutor = ctx.usuario()?;
        if resolutor == self.solicitante && !ctx.tiene(Permiso::AutoaprobarGasto) {
            return Err(ErrorDominio::SegregacionDeDeberes);
        }
        Ok(resolutor)
    }

    fn exigir_no_terminal(&self) -> Result<(), ErrorDominio> {
        if self.estado.es_terminal() {
            return Err(ErrorDominio::TransicionInvalida(format!(
                "la aprobación ya está {:?} y no se re-resuelve",
                self.estado
            )));
        }
        Ok(())
    }

    fn motivo_valido(motivo: &str) -> Result<String, ErrorDominio> {
        let m = motivo.trim();
        if m.is_empty() {
            return Err(ErrorDominio::Invalido("el motivo es obligatorio".into()));
        }
        Ok(m.to_string())
    }

    fn sellar(&mut self, resolutor: Uuid, motivo: Option<String>, ahora: Instante) {
        self.resolutor = Some(resolutor);
        self.resuelto_en = Some(ahora);
        self.motivo = motivo;
        self.actualizado = ahora;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceso::{Actor, Alcance, ContextoAcceso};
    use std::collections::BTreeSet;

    fn ctx(usuario: Uuid, permisos: &[Permiso], sucursal: Uuid) -> ContextoAcceso {
        ContextoAcceso::nuevo(
            Actor::Usuario(usuario),
            permisos.iter().copied().collect::<BTreeSet<_>>(),
            Alcance::de([sucursal]),
        )
    }

    #[test]
    fn aprobar_registra_autoria_y_es_terminal() {
        let suc = Uuid::new_v4();
        let cajero = Uuid::new_v4();
        let admin = Uuid::new_v4();
        let mut ap = Aprobacion::pendiente(Uuid::new_v4(), suc, cajero, Instante::ahora());
        ap.aprobar(
            &ctx(admin, &[Permiso::AutorizarGasto], suc),
            Instante::ahora(),
        )
        .unwrap();
        assert_eq!(ap.estado, EstadoAprobacion::Aprobada);
        assert_eq!(ap.resolutor, Some(admin));
        assert!(ap.autorizado());
        // no se re-resuelve
        assert!(
            ap.rechazar(
                &ctx(admin, &[Permiso::AutorizarGasto], suc),
                "x",
                Instante::ahora()
            )
            .is_err()
        );
    }

    #[test]
    fn rechazo_requiere_motivo() {
        let suc = Uuid::new_v4();
        let mut ap = Aprobacion::pendiente(Uuid::new_v4(), suc, Uuid::new_v4(), Instante::ahora());
        let c = ctx(Uuid::new_v4(), &[Permiso::AutorizarGasto], suc);
        assert!(ap.rechazar(&c, "   ", Instante::ahora()).is_err());
        ap.rechazar(&c, "fuera de política", Instante::ahora())
            .unwrap();
        assert_eq!(ap.estado, EstadoAprobacion::Rechazada);
        assert_eq!(ap.motivo.as_deref(), Some("fuera de política"));
    }

    #[test]
    fn cajero_no_autoriza() {
        let suc = Uuid::new_v4();
        let cajero = Uuid::new_v4();
        let mut ap = Aprobacion::pendiente(Uuid::new_v4(), suc, cajero, Instante::ahora());
        // el cajero no porta AutorizarGasto
        let c = ctx(cajero, &[Permiso::RegistrarGasto], suc);
        assert!(ap.aprobar(&c, Instante::ahora()).is_err());
    }

    #[test]
    fn segregacion_de_deberes() {
        let suc = Uuid::new_v4();
        let solicitante = Uuid::new_v4();
        let mut ap = Aprobacion::pendiente(Uuid::new_v4(), suc, solicitante, Instante::ahora());
        // el mismo solicitante con AutorizarGasto pero sin AutoaprobarGasto no puede
        let c = ctx(solicitante, &[Permiso::AutorizarGasto], suc);
        assert_eq!(
            ap.aprobar(&c, Instante::ahora()),
            Err(ErrorDominio::SegregacionDeDeberes)
        );
        // con AutoaprobarGasto sí
        let c2 = ctx(
            solicitante,
            &[Permiso::AutorizarGasto, Permiso::AutoaprobarGasto],
            suc,
        );
        assert!(ap.aprobar(&c2, Instante::ahora()).is_ok());
    }

    #[test]
    fn cancelar_lo_hace_un_aprobador_no_el_solicitante() {
        let suc = Uuid::new_v4();
        let solicitante = Uuid::new_v4();
        let mut ap = Aprobacion::pendiente(Uuid::new_v4(), suc, solicitante, Instante::ahora());
        // el solicitante no cancela lo suyo ni con autoaprobar
        let propio = ctx(
            solicitante,
            &[Permiso::AutorizarGasto, Permiso::AutoaprobarGasto],
            suc,
        );
        assert_eq!(
            ap.cancelar(&propio, "error", Instante::ahora()),
            Err(ErrorDominio::SegregacionDeDeberes)
        );
        // un aprobador distinto sí, con motivo
        let admin = ctx(Uuid::new_v4(), &[Permiso::AutorizarGasto], suc);
        ap.cancelar(&admin, "gasto por error", Instante::ahora())
            .unwrap();
        assert_eq!(ap.estado, EstadoAprobacion::Cancelada);
    }

    #[test]
    fn no_se_cancela_una_ya_resuelta() {
        let suc = Uuid::new_v4();
        let mut ap = Aprobacion::pendiente(Uuid::new_v4(), suc, Uuid::new_v4(), Instante::ahora());
        let admin = ctx(Uuid::new_v4(), &[Permiso::AutorizarGasto], suc);
        ap.aprobar(&admin, Instante::ahora()).unwrap();
        assert!(ap.cancelar(&admin, "tarde", Instante::ahora()).is_err());
    }
}
