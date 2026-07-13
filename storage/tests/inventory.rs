//! Pruebas de integración de `inventory-per-branch` (grupo 5 de `tasks.md`):
//! saldo vs. ledger, no-negatividad, reversión, ajuste, aislamiento, unidad por
//! tipo, materialización perezosa y concurrencia. Ejercen `domain` + `storage`.

use domain::acceso::{Alcance, ContextoAcceso, Permiso};
use domain::barcode::Ean13;
use domain::inventario::{Cantidad, EstadoMovimiento};
use domain::producto::{OrigenProducto, TipoProducto};
use domain::sucursal::{CodigoSucursal, TipoSucursal};
use domain::tiempo::ZonaHoraria;
use domain::unidades::Gramos;
use storage::prelude::*;
use uuid::Uuid;

fn url_temporal() -> String {
    let p = std::env::temp_dir().join(format!("toypos-inv-{}.db", Uuid::new_v4()));
    format!("sqlite://{}", p.display())
}

async fn con_admin() -> (Almacen, ContextoAcceso) {
    let alm = Almacen::abrir(&url_temporal()).await.unwrap();
    alm.bootstrap("MAT", "admin", "clave123").await.unwrap();
    let admin = alm.autenticar("admin", "clave123").await.unwrap();
    (alm, admin)
}

async fn expendio(alm: &Almacen, admin: &ContextoAcceso, codigo: &str) -> Uuid {
    alm.crear_sucursal(
        admin,
        TipoSucursal::Expendio,
        CodigoSucursal::nueva(codigo).unwrap(),
        codigo,
        ZonaHoraria::default(),
    )
    .await
    .unwrap()
    .id
}

/// Operador con permiso de inventario (ajustar + ver) acotado a una sucursal.
async fn operador_inv(
    alm: &Almacen,
    admin: &ContextoAcceso,
    nombre: &str,
    sucursal: Uuid,
) -> ContextoAcceso {
    let rol = alm
        .crear_rol(
            admin,
            &format!("inv-{nombre}"),
            &[Permiso::AjustarInventario, Permiso::VerInventario],
        )
        .await
        .unwrap();
    alm.crear_usuario(
        admin,
        nombre,
        "clave123",
        &[rol.id],
        Alcance::de([sucursal]),
    )
    .await
    .unwrap();
    alm.autenticar(nombre, "clave123").await.unwrap()
}

async fn producto_pieza(alm: &Almacen, admin: &ContextoAcceso, nombre: &str, gtin: &str) -> Uuid {
    alm.crear_producto(
        admin,
        BorradorProducto {
            nombre: nombre.into(),
            tipo: TipoProducto::Pieza,
            origen: OrigenProducto::Externo,
            codigo: AltaCodigo::Externo(Ean13::parse(gtin).unwrap()),
            peso_empaque: None,
        },
    )
    .await
    .unwrap()
    .id
}

async fn producto_peso(alm: &Almacen, admin: &ContextoAcceso, nombre: &str) -> Uuid {
    alm.crear_producto(
        admin,
        BorradorProducto {
            nombre: nombre.into(),
            tipo: TipoProducto::PesoVariable,
            origen: OrigenProducto::Matriz,
            codigo: AltaCodigo::Ninguno,
            peso_empaque: None,
        },
    )
    .await
    .unwrap()
    .id
}

/// Suma de los deltas de los movimientos **aplicados** (vigentes).
fn suma_aplicados(movs: &[domain::inventario::MovimientoInventario]) -> i64 {
    movs.iter()
        .filter(|m| m.estado == EstadoMovimiento::Aplicado)
        .map(|m| m.delta.magnitud())
        .sum()
}

// -------- 5.1 Saldo == suma de movimientos vigentes --------

#[tokio::test]
async fn saldo_iguala_la_suma_de_movimientos() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let prod = producto_pieza(&alm, &admin, "clavos", "7501059224827").await;
    let op = operador_inv(&alm, &admin, "ana", a).await;

    for objetivo in [50, 48, 60, 55] {
        alm.ajustar(&op, prod, a, Cantidad::Unidades(objetivo), "conteo")
            .await
            .unwrap();
    }
    let saldo = alm.existencia(&op, prod, a).await.unwrap();
    assert_eq!(saldo, Cantidad::Unidades(55)); // último objetivo (absoluto)
    let movs = alm.movimientos_de(&op, prod, a).await.unwrap();
    assert_eq!(saldo.magnitud(), suma_aplicados(&movs)); // saldo == ledger vigente
    assert_eq!(movs.len(), 4); // cuatro asientos, ninguno duplicado
}

// -------- 5.2 No negatividad --------

#[tokio::test]
async fn no_negatividad() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let prod = producto_pieza(&alm, &admin, "clavos", "7501059224827").await;
    let op = operador_inv(&alm, &admin, "ana", a).await;

    // Ajustar a un valor negativo se rechaza.
    assert!(
        alm.ajustar(&op, prod, a, Cantidad::Unidades(-1), "malo")
            .await
            .is_err()
    );
    // El saldo sigue en 0 (nada se aplicó).
    assert_eq!(
        alm.existencia(&op, prod, a).await.unwrap(),
        Cantidad::Unidades(0)
    );
}

// -------- 5.3 Reversión ida y vuelta, y reversión que dejaría negativo --------

#[tokio::test]
async fn reversion_ida_vuelta_y_no_negativa() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let prod = producto_pieza(&alm, &admin, "clavos", "7501059224827").await;
    let op = operador_inv(&alm, &admin, "ana", a).await;

    // Dos ajustes: 0→10 (m1), 10→3 (m2). Saldo 3.
    alm.ajustar(&op, prod, a, Cantidad::Unidades(10), "alta")
        .await
        .unwrap();
    alm.ajustar(&op, prod, a, Cantidad::Unidades(3), "bajó")
        .await
        .unwrap();
    let movs = alm.movimientos_de(&op, prod, a).await.unwrap();
    let m1 = movs.iter().find(|m| m.delta.magnitud() == 10).unwrap().id; // el +10
    let m2 = movs.iter().find(|m| m.delta.magnitud() == -7).unwrap().id; // el -7

    // Revertir m1 (+10) dejaría el saldo en 3 - 10 = -7 → rechazado (D25).
    assert!(alm.revertir_movimiento(&op, m1).await.is_err());
    assert_eq!(
        alm.existencia(&op, prod, a).await.unwrap(),
        Cantidad::Unidades(3)
    );

    // Revertir m2 (-7) sube el saldo a 10; rehacerlo lo devuelve a 3, sin duplicar.
    alm.revertir_movimiento(&op, m2).await.unwrap();
    assert_eq!(
        alm.existencia(&op, prod, a).await.unwrap(),
        Cantidad::Unidades(10)
    );
    alm.revertir_movimiento(&op, m2).await.unwrap(); // rehacer
    assert_eq!(
        alm.existencia(&op, prod, a).await.unwrap(),
        Cantidad::Unidades(3)
    );

    let movs = alm.movimientos_de(&op, prod, a).await.unwrap();
    assert_eq!(movs.len(), 2, "el toggle no apila copias");
    assert_eq!(
        alm.existencia(&op, prod, a).await.unwrap().magnitud(),
        suma_aplicados(&movs)
    );
}

// -------- 5.4 Ajuste: permiso, alcance y motivo --------

#[tokio::test]
async fn ajuste_exige_permiso_alcance_y_motivo() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let prod = producto_pieza(&alm, &admin, "clavos", "7501059224827").await;

    // Usuario sin AjustarInventario (solo VerInventario).
    let rol = alm
        .crear_rol(&admin, "solo-ve", &[Permiso::VerInventario])
        .await
        .unwrap();
    alm.crear_usuario(&admin, "beto", "clave123", &[rol.id], Alcance::de([a]))
        .await
        .unwrap();
    let beto = alm.autenticar("beto", "clave123").await.unwrap();
    assert!(
        alm.ajustar(&beto, prod, a, Cantidad::Unidades(5), "x")
            .await
            .is_err()
    );

    // Con permiso pero sin motivo, se rechaza.
    let op = operador_inv(&alm, &admin, "ana", a).await;
    assert!(
        alm.ajustar(&op, prod, a, Cantidad::Unidades(5), "   ")
            .await
            .is_err()
    );
    assert!(
        alm.ajustar(&op, prod, a, Cantidad::Unidades(5), "alta")
            .await
            .is_ok()
    );
}

// -------- 5.5 Aislamiento por alcance --------

#[tokio::test]
async fn aislamiento_por_alcance() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let b = expendio(&alm, &admin, "SNB").await;
    let prod = producto_pieza(&alm, &admin, "clavos", "7501059224827").await;
    let op_a = operador_inv(&alm, &admin, "ana", a).await; // alcance solo A

    // No puede ajustar ni consultar el inventario de B.
    assert!(
        alm.ajustar(&op_a, prod, b, Cantidad::Unidades(5), "intruso")
            .await
            .is_err()
    );
    assert!(alm.existencia(&op_a, prod, b).await.is_err());
    assert!(alm.movimientos_de(&op_a, prod, b).await.is_err());
    // Sí en A.
    assert!(
        alm.ajustar(&op_a, prod, a, Cantidad::Unidades(5), "propio")
            .await
            .is_ok()
    );
}

// -------- 5.6 Unidad por tipo de producto --------

#[tokio::test]
async fn unidad_por_tipo_de_producto() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let pieza = producto_pieza(&alm, &admin, "clavos", "7501059224827").await;
    let peso = producto_peso(&alm, &admin, "pollo").await;
    let op = operador_inv(&alm, &admin, "ana", a).await;

    // El peso-variable lleva gramos; el pieza, unidades.
    alm.ajustar(&op, peso, a, Cantidad::Gramos(Gramos::new(2500)), "recibo")
        .await
        .unwrap();
    assert_eq!(
        alm.existencia(&op, peso, a).await.unwrap(),
        Cantidad::Gramos(Gramos::new(2500))
    );
    alm.ajustar(&op, pieza, a, Cantidad::Unidades(12), "recibo")
        .await
        .unwrap();
    assert_eq!(
        alm.existencia(&op, pieza, a).await.unwrap(),
        Cantidad::Unidades(12)
    );

    // Unidad discordante (gramos para un pieza) se rechaza.
    assert!(
        alm.ajustar(&op, pieza, a, Cantidad::Gramos(Gramos::new(500)), "malo")
            .await
            .is_err()
    );
}

// -------- 5.7 Materialización perezosa --------

#[tokio::test]
async fn materializacion_perezosa() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let b = expendio(&alm, &admin, "SNB").await;
    let prod = producto_pieza(&alm, &admin, "clavos", "7501059224827").await;
    let op_a = operador_inv(&alm, &admin, "ana", a).await;
    let op_b = operador_inv(&alm, &admin, "beto", b).await;

    // Recién dado de alta, existencia 0 en toda sucursal (sin filas).
    assert_eq!(
        alm.existencia(&op_a, prod, a).await.unwrap(),
        Cantidad::Unidades(0)
    );
    assert_eq!(
        alm.existencia(&op_b, prod, b).await.unwrap(),
        Cantidad::Unidades(0)
    );
    assert!(alm.movimientos_de(&op_a, prod, a).await.unwrap().is_empty());

    // El primer movimiento materializa la fila solo de esa sucursal.
    alm.ajustar(&op_a, prod, a, Cantidad::Unidades(7), "alta")
        .await
        .unwrap();
    assert_eq!(
        alm.existencia(&op_a, prod, a).await.unwrap(),
        Cantidad::Unidades(7)
    );
    assert_eq!(
        alm.existencia(&op_b, prod, b).await.unwrap(),
        Cantidad::Unidades(0)
    );
}

// -------- 5.8 Concurrencia: ajustes simultáneos sin corromper el saldo --------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrencia_sin_corromper_el_saldo() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let op = operador_inv(&alm, &admin, "ana", a).await;
    // Cada tarea ajusta su propio producto a la vez (peso-variable, sin GTIN que colisione).
    let mut productos = Vec::new();
    for i in 0..6u32 {
        productos.push(producto_peso(&alm, &admin, &format!("prod{i}")).await);
    }

    let mut tareas = Vec::new();
    for (i, prod) in productos.into_iter().enumerate() {
        let alm = alm.clone();
        let op = op.clone();
        tareas.push(tokio::spawn(async move {
            let objetivo = Cantidad::Gramos(Gramos::new((i as i64 + 1) * 1000));
            alm.ajustar(&op, prod, a, objetivo, "concurrente").await?;
            let saldo = alm.existencia(&op, prod, a).await?;
            let movs = alm.movimientos_de(&op, prod, a).await?;
            // invariante: saldo == suma de movimientos vigentes
            assert_eq!(
                saldo.magnitud(),
                movs.iter()
                    .filter(|m| m.estado == EstadoMovimiento::Aplicado)
                    .map(|m| m.delta.magnitud())
                    .sum::<i64>()
            );
            storage::Resultado::Ok(())
        }));
    }
    for t in tareas {
        t.await
            .unwrap()
            .expect("cada ajuste concurrente es consistente");
    }
}
