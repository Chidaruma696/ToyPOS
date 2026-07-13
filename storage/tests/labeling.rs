//! Pruebas de integración de `labeling-expiry` (grupos 1 y 4 de `tasks.md`):
//! vida útil, identidad por etiqueta, cajas, inventario intacto (D34),
//! permisos/alcance, bandeja de notificaciones y barrido "por vencer" (D36).

use domain::acceso::{Alcance, ContextoAcceso, Permiso};
use domain::barcode::Ean13;
use domain::inventario::Cantidad;
use domain::notificacion::TipoNotificacion;
use domain::producto::{OrigenProducto, TipoProducto, VidaUtil};
use domain::sucursal::{CodigoSucursal, TipoSucursal};
use domain::tiempo::{Instante, ZonaHoraria};
use domain::unidades::Gramos;
use jiff::Span;
use jiff::civil::Date;
use storage::prelude::*;
use uuid::Uuid;

fn url_temporal() -> String {
    let p = std::env::temp_dir().join(format!("toypos-eti-{}.db", Uuid::new_v4()));
    format!("sqlite://{}", p.display())
}

async fn con_admin() -> (Almacen, ContextoAcceso, Uuid) {
    let alm = Almacen::abrir(&url_temporal()).await.unwrap();
    let (matriz, _) = alm.bootstrap("MAT", "admin", "clave123").await.unwrap();
    let admin = alm.autenticar("admin", "clave123").await.unwrap();
    (alm, admin, matriz.id)
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

/// Operador de etiquetado y bandeja, acotado a una sucursal.
async fn etiquetador(
    alm: &Almacen,
    admin: &ContextoAcceso,
    nombre: &str,
    sucursal: Uuid,
) -> ContextoAcceso {
    let rol = alm
        .crear_rol(
            admin,
            &format!("eti-{nombre}"),
            &[Permiso::Etiquetar, Permiso::VerNotificaciones],
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

async fn producto_peso(alm: &Almacen, admin: &ContextoAcceso, nombre: &str) -> Uuid {
    alm.crear_producto(
        admin,
        BorradorProducto {
            nombre: nombre.into(),
            tipo: TipoProducto::PesoVariable,
            origen: OrigenProducto::Matriz,
            codigo: AltaCodigo::Ninguno,
            peso_empaque: None,
            vida_util: None,
        },
    )
    .await
    .unwrap()
    .id
}

async fn producto_pieza(
    alm: &Almacen,
    admin: &ContextoAcceso,
    nombre: &str,
    origen: OrigenProducto,
) -> Uuid {
    let codigo = match origen {
        OrigenProducto::Matriz => AltaCodigo::GenerarMatriz,
        OrigenProducto::Externo => AltaCodigo::Externo(Ean13::parse("7501059224827").unwrap()),
    };
    alm.crear_producto(
        admin,
        BorradorProducto {
            nombre: nombre.into(),
            tipo: TipoProducto::Pieza,
            origen,
            codigo,
            peso_empaque: None,
            vida_util: None,
        },
    )
    .await
    .unwrap()
    .id
}

/// Un instante cuyo día local (zona default) cae `dias_antes` días antes de la
/// fecha dada — para situar el barrido cerca de una caducidad real.
fn instante_antes_de(fecha: Date, dias_antes: i64) -> Instante {
    let dia = fecha.checked_sub(Span::new().days(dias_antes)).unwrap();
    let zoned = dia.at(12, 0, 0, 0).in_tz("America/Mexico_City").unwrap();
    Instante::desde_timestamp(zoned.timestamp())
}

// -------- 1.3 Vida útil: default y edición --------

#[tokio::test]
async fn vida_util_default_y_edicion_con_permiso() {
    let (alm, admin, _) = con_admin().await;
    let prod = producto_pieza(&alm, &admin, "Nescafé", OrigenProducto::Externo).await;

    // Default 9 al alta (se observa vía lookup por código).
    let p = alm
        .producto_por_codigo(&admin, "7501059224827")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.vida_util.meses(), 9);

    // Edición con `gestionar_productos`.
    alm.fijar_vida_util(&admin, prod, VidaUtil::nueva(6).unwrap())
        .await
        .unwrap();
    let p = alm
        .producto_por_codigo(&admin, "7501059224827")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.vida_util.meses(), 6);

    // Sin el permiso se rechaza.
    let a = expendio(&alm, &admin, "SNA").await;
    let op = etiquetador(&alm, &admin, "ana", a).await;
    assert!(
        alm.fijar_vida_util(&op, prod, VidaUtil::nueva(3).unwrap())
            .await
            .is_err()
    );
}

// -------- 4.5 Identidad por etiqueta y lookup local --------

#[tokio::test]
async fn identidad_por_etiqueta_y_lookup() {
    let (alm, admin, matriz) = con_admin().await;
    let prod = producto_peso(&alm, &admin, "tripa de pollo").await;

    // Dos pesadas idénticas → discriminadores y códigos distintos.
    let etiquetas = alm
        .etiquetar_pesadas(
            &admin,
            prod,
            matriz,
            &[Gramos::new(1250), Gramos::new(1250)],
        )
        .await
        .unwrap();
    assert_eq!(etiquetas.len(), 2);
    assert_ne!(etiquetas[0].discriminador, etiquetas[1].discriminador);
    assert_ne!(etiquetas[0].codigo, etiquetas[1].codigo);

    // El barcode decodifica peso + discriminador y el lookup resuelve el resto.
    let e = &etiquetas[0];
    let (disc, peso) = e.codigo.decodificar_etiqueta().unwrap();
    assert_eq!(disc, e.discriminador);
    assert_eq!(peso, Gramos::new(1250));
    let hallada = alm
        .etiqueta_por_codigo(&admin, e.codigo.get())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hallada.producto, prod);
    assert_eq!(hallada.peso, Gramos::new(1250));
    assert_eq!(hallada.caducidad, e.caducidad);

    // Pesada fuera de rango se rechaza sin emitir etiqueta.
    assert!(
        alm.etiquetar_pesadas(&admin, prod, matriz, &[Gramos::new(100_000)])
            .await
            .is_err()
    );
}

// -------- 4.5 Etiquetar no altera el inventario (D34) --------

#[tokio::test]
async fn etiquetar_no_altera_inventario() {
    let (alm, admin, matriz) = con_admin().await;
    let prod = producto_peso(&alm, &admin, "pechuga").await;
    let pieza = producto_pieza(&alm, &admin, "queso 500 g", OrigenProducto::Matriz).await;

    let etiquetas = alm
        .etiquetar_pesadas(&admin, prod, matriz, &[Gramos::new(900), Gramos::new(1100)])
        .await
        .unwrap();
    let ids: Vec<Uuid> = etiquetas.iter().map(|e| e.id).collect();
    alm.cerrar_caja_pesadas(&admin, &ids).await.unwrap();
    alm.cerrar_caja_pieza(&admin, pieza, matriz, 24)
        .await
        .unwrap();

    // Ninguna existencia se movió: matriz produce sin llevar stock propio.
    assert_eq!(
        alm.existencia(&admin, prod, matriz).await.unwrap(),
        Cantidad::Gramos(Gramos::CERO)
    );
    assert_eq!(
        alm.existencia(&admin, pieza, matriz).await.unwrap(),
        Cantidad::Unidades(0)
    );
    assert!(
        alm.movimientos_de(&admin, prod, matriz)
            .await
            .unwrap()
            .is_empty()
    );
}

// -------- 4.5 Cajas: agrupación, pieza y caducidad por origen --------

#[tokio::test]
async fn cajas_agrupan_y_respetan_origen() {
    let (alm, admin, matriz) = con_admin().await;
    let peso = producto_peso(&alm, &admin, "tripa").await;

    let etiquetas = alm
        .etiquetar_pesadas(
            &admin,
            peso,
            matriz,
            &[Gramos::new(900), Gramos::new(1100), Gramos::new(500)],
        )
        .await
        .unwrap();
    let ids: Vec<Uuid> = etiquetas.iter().map(|e| e.id).collect();
    let caja = alm.cerrar_caja_pesadas(&admin, &ids).await.unwrap();

    // N etiquetas y peso total consultables.
    let dentro = alm.etiquetas_de_caja(&admin, caja.id).await.unwrap();
    assert_eq!(dentro.len(), 3);
    let total: i64 = dentro.iter().map(|e| e.peso.get()).sum();
    assert_eq!(total, 2500);
    // Una etiqueta ya encajada no entra en otra caja.
    assert!(alm.cerrar_caja_pesadas(&admin, &ids[..1]).await.is_err());

    // Pieza de matriz: caja con cantidad y caducidad propia.
    let matriz_pieza = producto_pieza(&alm, &admin, "queso 500 g", OrigenProducto::Matriz).await;
    let caja = alm
        .cerrar_caja_pieza(&admin, matriz_pieza, matriz, 24)
        .await
        .unwrap();
    assert_eq!(caja.cantidad, Some(24));
    assert!(caja.caducidad.is_some());

    // Pieza externa: conserva la caducidad de su fábrica (sin caducidad nuestra).
    let externo = producto_pieza(&alm, &admin, "Nescafé", OrigenProducto::Externo).await;
    let caja = alm
        .cerrar_caja_pieza(&admin, externo, matriz, 12)
        .await
        .unwrap();
    assert_eq!(caja.caducidad, None);
}

// -------- 4.5 La vida útil editada no reetiqueta lo emitido --------

#[tokio::test]
async fn caducidad_congelada_ante_cambio_de_vida_util() {
    let (alm, admin, matriz) = con_admin().await;
    let prod = producto_peso(&alm, &admin, "pollo").await;

    let antes = alm
        .etiquetar_pesadas(&admin, prod, matriz, &[Gramos::new(800)])
        .await
        .unwrap();
    alm.fijar_vida_util(&admin, prod, VidaUtil::nueva(1).unwrap())
        .await
        .unwrap();
    let despues = alm
        .etiquetar_pesadas(&admin, prod, matriz, &[Gramos::new(800)])
        .await
        .unwrap();

    // La nueva usa 1 mes; la emitida conserva sus 9 meses.
    assert!(despues[0].caducidad < antes[0].caducidad);
    let conservada = alm
        .etiqueta_por_codigo(&admin, antes[0].codigo.get())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conservada.caducidad, antes[0].caducidad);
}

// -------- 4.5 Permisos y alcance en etiquetado y bandeja --------

#[tokio::test]
async fn permisos_y_alcance() {
    let (alm, admin, matriz) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let b = expendio(&alm, &admin, "SNB").await;
    let prod = producto_peso(&alm, &admin, "tripa").await;
    let op_a = etiquetador(&alm, &admin, "ana", a).await;

    // Etiquetar fuera del alcance (o sin permiso) se rechaza.
    assert!(
        alm.etiquetar_pesadas(&op_a, prod, b, &[Gramos::new(500)])
            .await
            .is_err()
    );
    let rol = alm
        .crear_rol(&admin, "sin-eti", &[Permiso::Vender])
        .await
        .unwrap();
    alm.crear_usuario(&admin, "beto", "clave123", &[rol.id], Alcance::de([a]))
        .await
        .unwrap();
    let beto = alm.autenticar("beto", "clave123").await.unwrap();
    assert!(
        alm.etiquetar_pesadas(&beto, prod, a, &[Gramos::new(500)])
            .await
            .is_err()
    );

    // El lookup no revela etiquetas de sucursales fuera del alcance.
    let em = alm
        .etiquetar_pesadas(&admin, prod, matriz, &[Gramos::new(700)])
        .await
        .unwrap();
    assert!(
        alm.etiqueta_por_codigo(&op_a, em[0].codigo.get())
            .await
            .unwrap()
            .is_none()
    );

    // Bandeja: fuera del alcance o sin permiso se rechaza.
    assert!(alm.bandeja(&op_a, b).await.is_err());
    assert!(alm.bandeja(&beto, a).await.is_err());
    assert!(alm.bandeja(&op_a, a).await.unwrap().is_empty());
}

// -------- 4.5 Notificaciones: emisión del sistema y marcar leída --------

#[tokio::test]
async fn notificaciones_emision_y_leida_idempotente() {
    let (alm, admin, _) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let op = etiquetador(&alm, &admin, "ana", a).await;

    // Solo el sistema emite; un usuario no.
    assert!(
        alm.emitir(&admin, TipoNotificacion::PorVencer, "aviso", a)
            .await
            .is_err()
    );
    let sistema = ContextoAcceso::sistema();
    let n = alm
        .emitir(&sistema, TipoNotificacion::PorVencer, "aviso operativo", a)
        .await
        .unwrap();

    // Aparece no leída en la bandeja; marcar leída es idempotente y conserva la fila.
    let bandeja = alm.bandeja(&op, a).await.unwrap();
    assert_eq!(bandeja.len(), 1);
    assert!(!bandeja[0].leida());
    alm.marcar_leida(&op, n.id).await.unwrap();
    let bandeja = alm.bandeja(&op, a).await.unwrap();
    assert_eq!(bandeja.len(), 1, "marcar leída conserva la notificación");
    assert!(bandeja[0].leida());
    let marca = bandeja[0].leida_en;
    alm.marcar_leida(&op, n.id).await.unwrap(); // segunda vez: sin cambio
    assert_eq!(alm.bandeja(&op, a).await.unwrap()[0].leida_en, marca);
}

// -------- 4.5 Barrido "por vencer": agrupación, doble destino, idempotencia --------

#[tokio::test]
async fn alerta_por_vencer_agrupada_y_doble_destino() {
    let (alm, admin, matriz) = con_admin().await;
    let exp = expendio(&alm, &admin, "SNA").await;
    let op = etiquetador(&alm, &admin, "ana", exp).await;
    let prod = producto_peso(&alm, &admin, "tripa de pollo").await;

    // Siembra: 3 etiquetas del producto en el expendio (el envío hará esto después).
    let etiquetas = alm
        .etiquetar_pesadas(
            &op,
            prod,
            exp,
            &[Gramos::new(900), Gramos::new(1100), Gramos::new(500)],
        )
        .await
        .unwrap();
    let caducidad = etiquetas[0].caducidad;
    let sistema = ContextoAcceso::sistema();

    // Fuera de la ventana (hoy, a 9 meses de caducar): nada.
    assert_eq!(
        alm.barrer_por_vencer(&sistema, Instante::ahora())
            .await
            .unwrap(),
        0
    );

    // A 3 días de caducar: UNA notificación agrupada por producto… en DOS bandejas.
    let cerca = instante_antes_de(caducidad, 3);
    assert_eq!(alm.barrer_por_vencer(&sistema, cerca).await.unwrap(), 2);
    let en_expendio = alm.bandeja(&op, exp).await.unwrap();
    assert_eq!(en_expendio.len(), 1, "agrupada: una por producto, no tres");
    assert!(en_expendio[0].mensaje.contains("tripa de pollo"));
    assert!(en_expendio[0].mensaje.contains("por vencer"));
    let en_matriz = alm.bandeja(&admin, matriz).await.unwrap();
    assert_eq!(en_matriz.len(), 1, "la administración también se entera");

    // Re-barrer no duplica (idempotente), ni siquiera tras marcar leída.
    assert_eq!(alm.barrer_por_vencer(&sistema, cerca).await.unwrap(), 0);
    alm.marcar_leida(&op, en_expendio[0].id).await.unwrap();
    assert_eq!(alm.barrer_por_vencer(&sistema, cerca).await.unwrap(), 0);
    assert_eq!(alm.bandeja(&op, exp).await.unwrap().len(), 1);

    // El barrido es del sistema: un usuario no lo dispara.
    assert!(alm.barrer_por_vencer(&admin, cerca).await.is_err());
}

#[tokio::test]
async fn alerta_ignora_matriz_e_incluye_cajas_de_pieza() {
    let (alm, admin, matriz) = con_admin().await;
    let exp = expendio(&alm, &admin, "SNB").await;
    let peso = producto_peso(&alm, &admin, "pechuga").await;
    let pieza = producto_pieza(&alm, &admin, "queso 500 g", OrigenProducto::Matriz).await;
    let sistema = ContextoAcceso::sistema();

    // Stock por vencer EN MATRIZ: el barrido no lo alerta (D36).
    let en_matriz = alm
        .etiquetar_pesadas(&admin, peso, matriz, &[Gramos::new(800)])
        .await
        .unwrap();
    let cerca = instante_antes_de(en_matriz[0].caducidad, 3);
    assert_eq!(alm.barrer_por_vencer(&sistema, cerca).await.unwrap(), 0);

    // Cajas de pieza en el expendio: alertan sumando cantidades.
    alm.cerrar_caja_pieza(&admin, pieza, exp, 24).await.unwrap();
    let caja = alm.cerrar_caja_pieza(&admin, pieza, exp, 16).await.unwrap();
    let cerca = instante_antes_de(caja.caducidad.unwrap(), 3);
    assert_eq!(alm.barrer_por_vencer(&sistema, cerca).await.unwrap(), 2);
    let bandeja = alm.bandeja(&admin, exp).await.unwrap();
    assert_eq!(bandeja.len(), 1);
    assert!(bandeja[0].mensaje.contains("40 pzas queso 500 g"));
}
