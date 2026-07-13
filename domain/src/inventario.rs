//! Inventario: existencia por `producto × sucursal` como ledger de **movimientos**
//! tipados (append-only, reversibles D15) del que se deriva el saldo.
//!
//! La **cantidad** se expresa en la unidad natural del producto (D6/D24):
//! `pieza` → unidades enteras, `peso_variable` → gramos; el tipo impide
//! mezclarlas. La existencia **nunca es negativa** (D25). El ajuste es
//! **absoluto**: fija la existencia a la cantidad contada (D26).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::acceso::{ContextoAcceso, Permiso};
use crate::error::ErrorDominio;
use crate::producto::{Producto, UnidadVenta};
use crate::tiempo::Instante;
use crate::unidades::Gramos;

/// Cantidad de existencia o de un movimiento, tipada por la unidad del producto.
/// Puede ser negativa (un delta de salida); la **existencia** que resulta no.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cantidad {
    /// Producto `pieza`: unidades enteras.
    Unidades(i64),
    /// Producto `peso_variable`: gramos (D6).
    Gramos(Gramos),
}

impl Cantidad {
    /// Cantidad cero en la unidad dada (existencia sin movimientos).
    #[must_use]
    pub fn cero(unidad: UnidadVenta) -> Cantidad {
        match unidad {
            UnidadVenta::Unidad => Cantidad::Unidades(0),
            UnidadVenta::Kilogramo => Cantidad::Gramos(Gramos::CERO),
        }
    }

    /// Construye una cantidad en la unidad dada a partir de su magnitud entera.
    #[must_use]
    pub fn en(unidad: UnidadVenta, magnitud: i64) -> Cantidad {
        match unidad {
            UnidadVenta::Unidad => Cantidad::Unidades(magnitud),
            UnidadVenta::Kilogramo => Cantidad::Gramos(Gramos::new(magnitud)),
        }
    }

    #[must_use]
    pub fn unidad(self) -> UnidadVenta {
        match self {
            Cantidad::Unidades(_) => UnidadVenta::Unidad,
            Cantidad::Gramos(_) => UnidadVenta::Kilogramo,
        }
    }

    /// Magnitud entera con signo (unidades o gramos, según la unidad).
    #[must_use]
    pub fn magnitud(self) -> i64 {
        match self {
            Cantidad::Unidades(n) => n,
            Cantidad::Gramos(g) => g.get(),
        }
    }

    #[must_use]
    pub fn es_negativa(self) -> bool {
        self.magnitud() < 0
    }

    #[must_use]
    pub fn es_cero(self) -> bool {
        self.magnitud() == 0
    }

    /// La cantidad opuesta (para deshacer un movimiento), misma unidad.
    #[must_use]
    pub fn negada(self) -> Cantidad {
        Cantidad::en(self.unidad(), -self.magnitud())
    }

    /// Suma dos cantidades de la **misma unidad**; rechaza mezclar unidades con gramos.
    pub fn sumar(self, otra: Cantidad) -> Result<Cantidad, ErrorDominio> {
        if self.unidad() != otra.unidad() {
            return Err(ErrorDominio::Regla(
                "no se pueden mezclar unidades con gramos en el inventario".into(),
            ));
        }
        Ok(Cantidad::en(
            self.unidad(),
            self.magnitud() + otra.magnitud(),
        ))
    }

    /// Resta `otra` de `self` (misma unidad).
    pub fn restar(self, otra: Cantidad) -> Result<Cantidad, ErrorDominio> {
        self.sumar(otra.negada())
    }

    /// ¿La cantidad se expresa en la unidad de venta del producto?
    #[must_use]
    pub fn concuerda_con(self, producto: &Producto) -> bool {
        self.unidad() == producto.unidad_venta()
    }
}

/// Tipo de movimiento. Extensible: `Ajuste` lo postea el conteo manual (D26),
/// `Envio` y `Recepcion` los postea la capacidad de envíos (D40) y `Venta`
/// queda reservado para el POS futuro. La **reversión no es un tipo**: es el
/// `EstadoMovimiento` del propio movimiento (toggle, sin apilar copias, D15/D27).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoMovimiento {
    Ajuste,
    Envio,
    Recepcion,
    Venta,
}

/// Estado de un movimiento: aplicado cuenta en el saldo; revertido no (D27/D15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoMovimiento {
    Aplicado,
    Revertido,
}

/// Un asiento del ledger de inventario. Append-only; su `estado` se togglea al
/// revertir (no se apilan copias, D15). El `delta` lleva signo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovimientoInventario {
    pub id: Uuid,
    pub producto: Uuid,
    pub sucursal: Uuid,
    pub tipo: TipoMovimiento,
    pub delta: Cantidad,
    pub motivo: Option<String>,
    pub estado: EstadoMovimiento,
    pub registrado: Instante,
    pub actualizado: Instante,
}

impl MovimientoInventario {
    /// Construye el movimiento de un **ajuste absoluto**: fija la existencia a
    /// `objetivo` (la cantidad contada). Exige `AjustarInventario` + alcance, que
    /// la unidad concuerde con el producto, que el objetivo no sea negativo (D25)
    /// y un motivo no vacío (D26). El `delta` guardado es `objetivo − saldo`.
    pub fn ajuste(
        ctx: &ContextoAcceso,
        producto: &Producto,
        sucursal: Uuid,
        objetivo: Cantidad,
        saldo_actual: Cantidad,
        motivo: &str,
        ahora: Instante,
    ) -> Result<Self, ErrorDominio> {
        ctx.requiere_en(Permiso::AjustarInventario, sucursal)?;
        if !objetivo.concuerda_con(producto) {
            return Err(ErrorDominio::Regla(
                "la cantidad no está en la unidad del producto".into(),
            ));
        }
        if objetivo.es_negativa() {
            return Err(ErrorDominio::Regla(
                "la existencia no puede ser negativa".into(),
            ));
        }
        let motivo = motivo.trim();
        if motivo.is_empty() {
            return Err(ErrorDominio::Invalido(
                "el motivo del ajuste es obligatorio".into(),
            ));
        }
        let delta = objetivo.restar(saldo_actual)?;
        Ok(MovimientoInventario {
            id: Uuid::new_v4(),
            producto: producto.id,
            sucursal,
            tipo: TipoMovimiento::Ajuste,
            delta,
            motivo: Some(motivo.to_string()),
            estado: EstadoMovimiento::Aplicado,
            registrado: ahora,
            actualizado: ahora,
        })
    }

    /// La compensación de saldo que produce alternar la reversión, **sin** mutar:
    /// si está aplicado, quitar su efecto (`−delta`); si revertido, reaplicarlo (`+delta`).
    #[must_use]
    pub fn compensacion(&self) -> Cantidad {
        match self.estado {
            EstadoMovimiento::Aplicado => self.delta.negada(),
            EstadoMovimiento::Revertido => self.delta,
        }
    }

    /// Togglea el estado `aplicado ⇄ revertido` (tras validar el saldo). No
    /// duplica la entidad; cada alternancia se audita en la capa de datos.
    pub fn alternar_reversion(&mut self, ahora: Instante) {
        self.estado = match self.estado {
            EstadoMovimiento::Aplicado => EstadoMovimiento::Revertido,
            EstadoMovimiento::Revertido => EstadoMovimiento::Aplicado,
        };
        self.actualizado = ahora;
    }
}

/// Existencia de un producto en una sucursal. Su cantidad nunca es negativa (D25).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Existencia {
    pub producto: Uuid,
    pub sucursal: Uuid,
    pub cantidad: Cantidad,
}

impl Existencia {
    #[must_use]
    pub fn nueva(producto: Uuid, sucursal: Uuid, cantidad: Cantidad) -> Self {
        Existencia {
            producto,
            sucursal,
            cantidad,
        }
    }

    /// Existencia cero (materialización perezosa, D30).
    #[must_use]
    pub fn cero(producto: Uuid, sucursal: Uuid, unidad: UnidadVenta) -> Self {
        Existencia::nueva(producto, sucursal, Cantidad::cero(unidad))
    }

    /// Aplica un delta al saldo (misma unidad) y rechaza que quede negativo (D25).
    pub fn aplicar(&self, delta: Cantidad) -> Result<Existencia, ErrorDominio> {
        let cantidad = self.cantidad.sumar(delta)?;
        if cantidad.es_negativa() {
            return Err(ErrorDominio::Regla(
                "la existencia no puede ser negativa".into(),
            ));
        }
        Ok(Existencia { cantidad, ..*self })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceso::{Actor, Alcance, ContextoAcceso};
    use crate::producto::{OrigenProducto, TipoProducto};

    fn producto_pieza() -> Producto {
        Producto::nuevo(
            "clavos",
            TipoProducto::Pieza,
            OrigenProducto::Externo,
            Some(crate::producto::CodigoBarras::Externo(
                crate::barcode::Ean13::parse("7501059224827").unwrap(),
            )),
            None,
            Instante::ahora(),
        )
        .unwrap()
    }

    fn ctx(permisos: &[Permiso], sucursal: Uuid) -> ContextoAcceso {
        ContextoAcceso::nuevo(
            Actor::Usuario(Uuid::new_v4()),
            permisos.iter().copied().collect(),
            Alcance::de([sucursal]),
        )
    }

    #[test]
    fn no_se_mezclan_unidades_con_gramos() {
        let r = Cantidad::Unidades(3).sumar(Cantidad::Gramos(Gramos::new(500)));
        assert!(r.is_err());
    }

    #[test]
    fn cero_deriva_de_la_unidad() {
        assert_eq!(Cantidad::cero(UnidadVenta::Unidad), Cantidad::Unidades(0));
        assert_eq!(
            Cantidad::cero(UnidadVenta::Kilogramo),
            Cantidad::Gramos(Gramos::CERO)
        );
    }

    #[test]
    fn ajuste_absoluto_calcula_el_delta() {
        let p = producto_pieza();
        let suc = Uuid::new_v4();
        let c = ctx(&[Permiso::AjustarInventario], suc);
        // saldo 10, objetivo 15 → delta +5
        let m = MovimientoInventario::ajuste(
            &c,
            &p,
            suc,
            Cantidad::Unidades(15),
            Cantidad::Unidades(10),
            "conteo",
            Instante::ahora(),
        )
        .unwrap();
        assert_eq!(m.delta, Cantidad::Unidades(5));
        assert_eq!(m.tipo, TipoMovimiento::Ajuste);
    }

    #[test]
    fn ajuste_exige_permiso_motivo_y_no_negativo() {
        let p = producto_pieza();
        let suc = Uuid::new_v4();
        let sin = ctx(&[Permiso::Vender], suc);
        assert!(
            MovimientoInventario::ajuste(
                &sin,
                &p,
                suc,
                Cantidad::Unidades(5),
                Cantidad::Unidades(0),
                "x",
                Instante::ahora()
            )
            .is_err()
        );
        let con = ctx(&[Permiso::AjustarInventario], suc);
        // sin motivo
        assert!(
            MovimientoInventario::ajuste(
                &con,
                &p,
                suc,
                Cantidad::Unidades(5),
                Cantidad::Unidades(0),
                "  ",
                Instante::ahora()
            )
            .is_err()
        );
        // objetivo negativo
        assert!(
            MovimientoInventario::ajuste(
                &con,
                &p,
                suc,
                Cantidad::Unidades(-1),
                Cantidad::Unidades(0),
                "x",
                Instante::ahora()
            )
            .is_err()
        );
        // unidad discordante (gramos para un pieza)
        assert!(
            MovimientoInventario::ajuste(
                &con,
                &p,
                suc,
                Cantidad::Gramos(Gramos::new(500)),
                Cantidad::Unidades(0),
                "x",
                Instante::ahora()
            )
            .is_err()
        );
    }

    #[test]
    fn existencia_no_puede_quedar_negativa() {
        let e = Existencia::nueva(Uuid::new_v4(), Uuid::new_v4(), Cantidad::Unidades(10));
        assert!(e.aplicar(Cantidad::Unidades(-12)).is_err());
        assert_eq!(
            e.aplicar(Cantidad::Unidades(-3)).unwrap().cantidad,
            Cantidad::Unidades(7)
        );
    }

    #[test]
    fn reversion_alterna_y_compensa_sin_duplicar() {
        let suc = Uuid::new_v4();
        let c = ctx(&[Permiso::AjustarInventario], suc);
        let p = producto_pieza();
        let mut m = MovimientoInventario::ajuste(
            &c,
            &p,
            suc,
            Cantidad::Unidades(15),
            Cantidad::Unidades(10),
            "conteo",
            Instante::ahora(),
        )
        .unwrap();
        // aplicado: compensar = -delta
        assert_eq!(m.compensacion(), Cantidad::Unidades(-5));
        m.alternar_reversion(Instante::ahora());
        assert_eq!(m.estado, EstadoMovimiento::Revertido);
        // revertido: compensar = +delta (rehacer)
        assert_eq!(m.compensacion(), Cantidad::Unidades(5));
        m.alternar_reversion(Instante::ahora());
        assert_eq!(m.estado, EstadoMovimiento::Aplicado);
    }
}
