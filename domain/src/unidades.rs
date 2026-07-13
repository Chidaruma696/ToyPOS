//! Unidades enteras extremo a extremo — nunca `float` (D6).
//!
//! - Dinero en **centavos** (2 decimales).
//! - Peso en **gramos** (3 decimales de kg, precisión de la báscula Torrey).
//!
//! El peso viaja como el mismo entero desde la báscula hasta el cuadre físico,
//! sin redondearse jamás a 2 decimales (esa es la causa clásica del desfase).

use std::fmt;
use std::ops::{Add, Sub};

use serde::{Deserialize, Serialize};

/// Importe monetario entero en **centavos**.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Centavos(i64);

impl Centavos {
    pub const CERO: Centavos = Centavos(0);

    #[must_use]
    pub const fn new(centavos: i64) -> Self {
        Centavos(centavos)
    }

    /// Construye desde una cantidad de pesos y centavos (p. ej. `de_pesos(12, 40)` = $12.40).
    #[must_use]
    pub const fn de_pesos(pesos: i64, centavos: i64) -> Self {
        Centavos(pesos * 100 + centavos)
    }

    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }

    /// Importe de una venta por peso: `precio/kg × gramos ÷ 1000`, redondeado a
    /// centavos **medio hacia arriba** (D6). `self` es el precio por kilogramo.
    ///
    /// El efectivo siempre queda a 2 decimales; el redondeo ocurre por línea de
    /// venta, y el total es la suma de líneas ya redondeadas.
    #[must_use]
    pub fn importe_por_peso(self, peso: Gramos) -> Centavos {
        debug_assert!(
            self.0 >= 0 && peso.0 >= 0,
            "importe por peso asume valores no negativos"
        );
        let bruto = self.0 * peso.0; // centavos·gramo
        Centavos((bruto + 500) / 1000) // ÷1000 con medio-arriba (no negativo)
    }
}

impl Add for Centavos {
    type Output = Centavos;
    fn add(self, otro: Centavos) -> Centavos {
        Centavos(self.0 + otro.0)
    }
}

impl Sub for Centavos {
    type Output = Centavos;
    fn sub(self, otro: Centavos) -> Centavos {
        Centavos(self.0 - otro.0)
    }
}

impl std::iter::Sum for Centavos {
    fn sum<I: Iterator<Item = Centavos>>(iter: I) -> Centavos {
        iter.fold(Centavos::CERO, Add::add)
    }
}

impl fmt::Display for Centavos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let signo = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.abs();
        write!(f, "{signo}{}.{:02}", abs / 100, abs % 100)
    }
}

/// Peso entero en **gramos** (= 3 decimales de kilogramo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Gramos(i64);

impl Gramos {
    pub const CERO: Gramos = Gramos(0);

    #[must_use]
    pub const fn new(gramos: i64) -> Self {
        Gramos(gramos)
    }

    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl Add for Gramos {
    type Output = Gramos;
    fn add(self, otro: Gramos) -> Gramos {
        Gramos(self.0 + otro.0)
    }
}

impl fmt::Display for Gramos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:03} kg", self.0 / 1000, self.0.abs() % 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importe_por_peso_redondea_medio_arriba() {
        // 100.00/kg × 0.125 kg = 12.50 exacto
        let precio = Centavos::de_pesos(100, 0);
        assert_eq!(
            precio.importe_por_peso(Gramos::new(125)),
            Centavos::de_pesos(12, 50)
        );
    }

    #[test]
    fn importe_por_peso_redondeo_hacia_arriba_en_medio() {
        // 100.01/kg × 0.005 kg = 0.50005 → 0.50 ; probamos un medio exacto:
        // 1.00/kg × 0.5 g = 0.05 centavos·... usemos 100.00/kg × 1 g = 0.1 cent → 0?
        // 100.00/kg = 10000 cent/kg; × 1 g = 10000 cent·g; ÷1000 = 10 → 0.10. exacto.
        // medio: 15 cent·... construyamos: 5 cent/kg? mejor: bruto=1500 → (1500+500)/1000=2
        let precio = Centavos::new(1500); // arbitraria para forzar bruto con .5
        // 1500 cent/kg × 1 g = 1500 ; (1500+500)/1000 = 2
        assert_eq!(precio.importe_por_peso(Gramos::new(1)), Centavos::new(2));
    }

    #[test]
    fn display_centavos() {
        assert_eq!(Centavos::de_pesos(12, 40).to_string(), "12.40");
        assert_eq!(Centavos::new(5).to_string(), "0.05");
        assert_eq!(Centavos::new(-50).to_string(), "-0.50");
    }

    #[test]
    fn suma_de_lineas() {
        let total: Centavos = [Centavos::new(1240), Centavos::new(500)].into_iter().sum();
        assert_eq!(total, Centavos::new(1740));
    }
}
