//! Precios por `producto × sucursal × nivel`. "Congelado" es un estado
//! **derivado** de la ausencia de precio de menudeo vigente, no un flag (D5).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::tiempo::Instante;
use crate::unidades::Centavos;

/// Nivel de precio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Nivel {
    Menudeo,
    MedioMayoreo,
    Mayoreo,
}

impl Nivel {
    pub const TODOS: [Nivel; 3] = [Nivel::Menudeo, Nivel::MedioMayoreo, Nivel::Mayoreo];
}

/// Precio vigente para una combinación. A lo sumo uno por `(producto, sucursal,
/// nivel)`; la tabla se sobrescribe (el historial se reconstruye de la bitácora).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Precio {
    pub producto: Uuid,
    pub sucursal: Uuid,
    pub nivel: Nivel,
    pub monto: Centavos,
    pub actualizado: Instante,
}

/// ¿El producto está **congelado** (no vendible) en la sucursal? Lo está cuando
/// carece de precio de menudeo vigente (piso menudeo).
#[must_use]
pub fn esta_congelado(niveles_con_precio: &BTreeSet<Nivel>) -> bool {
    !niveles_con_precio.contains(&Nivel::Menudeo)
}

/// ¿Se puede vender en ese nivel? Hay precio para el nivel y no está congelado.
#[must_use]
pub fn vendible(niveles_con_precio: &BTreeSet<Nivel>, nivel: Nivel) -> bool {
    !esta_congelado(niveles_con_precio) && niveles_con_precio.contains(&nivel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_menudeo_esta_congelado() {
        let solo_mayoreo: BTreeSet<Nivel> = [Nivel::Mayoreo].into_iter().collect();
        assert!(esta_congelado(&solo_mayoreo));
        assert!(!vendible(&solo_mayoreo, Nivel::Mayoreo)); // congelado ⇒ nada vendible
    }

    #[test]
    fn con_menudeo_se_descongela() {
        let con_menudeo: BTreeSet<Nivel> = [Nivel::Menudeo, Nivel::Mayoreo].into_iter().collect();
        assert!(!esta_congelado(&con_menudeo));
        assert!(vendible(&con_menudeo, Nivel::Menudeo));
        assert!(vendible(&con_menudeo, Nivel::Mayoreo));
        assert!(!vendible(&con_menudeo, Nivel::MedioMayoreo)); // ese nivel no tiene precio
    }
}
