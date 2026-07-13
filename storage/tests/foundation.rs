//! Pruebas de integración de la fundación (sección 8 de `tasks.md`): aislamiento
//! por alcance, RBAC, precios, ciclo de aprobación, corte, auditoría,
//! concurrencia e identificadores generados. Ejercen `domain` + `storage` juntos.

use std::collections::BTreeSet;

use domain::acceso::{Alcance, ContextoAcceso, Permiso};
use domain::barcode::Ean13;
use domain::caja::ResumenVentas;
use domain::precio::Nivel;
use domain::producto::{OrigenProducto, TipoProducto};
use domain::sucursal::{CodigoSucursal, TipoSucursal};
use domain::tiempo::ZonaHoraria;
use domain::unidades::Centavos;
use storage::prelude::*;
use uuid::Uuid;

fn url_temporal() -> String {
    let p = std::env::temp_dir().join(format!("toypos-test-{}.db", Uuid::new_v4()));
    format!("sqlite://{}", p.display())
}

async fn nuevo() -> Almacen {
    Almacen::abrir(&url_temporal()).await.unwrap()
}

/// Bootstrap + admin autenticado.
async fn con_admin() -> (Almacen, ContextoAcceso) {
    let alm = nuevo().await;
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

async fn cajero_en(
    alm: &Almacen,
    admin: &ContextoAcceso,
    nombre: &str,
    sucursal: Uuid,
) -> ContextoAcceso {
    let rol = alm
        .crear_rol(
            admin,
            &format!("cajero-{nombre}"),
            &[
                Permiso::Vender,
                Permiso::RegistrarGasto,
                Permiso::OperarCaja,
            ],
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

async fn producto_pieza_externo(
    alm: &Almacen,
    admin: &ContextoAcceso,
    nombre: &str,
    gtin: &str,
) -> Uuid {
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

// -------- 8.1 Aislamiento por alcance --------

#[tokio::test]
async fn aislamiento_por_alcance() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let b = expendio(&alm, &admin, "SNB").await;
    let cajero_a = cajero_en(&alm, &admin, "ana", a).await;

    // No puede registrar un gasto en B (fuera de su alcance).
    let sesion = alm
        .abrir_sesion(&cajero_a, a, Centavos::de_pesos(200, 0))
        .await
        .unwrap();
    let escribe_b = alm
        .registrar_gasto(&cajero_a, b, sesion.id, Centavos::de_pesos(50, 0), "x")
        .await;
    assert!(escribe_b.is_err(), "un cajero de A no debe escribir en B");

    // No puede leer los gastos de B.
    assert!(
        alm.listar_gastos(&cajero_a, b).await.is_err(),
        "un cajero de A no debe leer B"
    );
    // Sí lee los suyos.
    assert!(alm.listar_gastos(&cajero_a, a).await.is_ok());
}

// -------- 8.2 RBAC: permiso concede su capacidad; agregar en caliente --------

#[tokio::test]
async fn rbac_permiso_exacto_y_en_caliente() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let prod = producto_pieza_externo(&alm, &admin, "Nescafé", "7501059224827").await;

    // Rol sin `editar_precio`.
    let rol = alm
        .crear_rol(&admin, "solo-vende", &[Permiso::Vender])
        .await
        .unwrap();
    alm.crear_usuario(&admin, "beto", "clave123", &[rol.id], Alcance::de([a]))
        .await
        .unwrap();
    let beto = alm.autenticar("beto", "clave123").await.unwrap();

    // Sin el permiso, no puede fijar precio.
    assert!(
        alm.fijar_precio(&beto, prod, a, Nivel::Menudeo, Centavos::de_pesos(40, 0))
            .await
            .is_err()
    );

    // El admin agrega el permiso al rol EN CALIENTE.
    alm.agregar_permiso_a_rol(&admin, rol.id, Permiso::EditarPrecio)
        .await
        .unwrap();

    // Al re-autenticar, beto ya porta la capacidad.
    let beto2 = alm.autenticar("beto", "clave123").await.unwrap();
    assert!(
        alm.fijar_precio(&beto2, prod, a, Nivel::Menudeo, Centavos::de_pesos(40, 0))
            .await
            .is_ok()
    );

    // Y QUITAR el permiso en caliente lo revoca.
    alm.quitar_permiso_a_rol(&admin, rol.id, Permiso::EditarPrecio)
        .await
        .unwrap();
    let beto3 = alm.autenticar("beto", "clave123").await.unwrap();
    assert!(
        alm.fijar_precio(&beto3, prod, a, Nivel::Menudeo, Centavos::de_pesos(45, 0))
            .await
            .is_err()
    );
}

// -------- 8.3 Precios: por sucursal, copiar, congelado/descongelado --------

#[tokio::test]
async fn precios_por_sucursal_copiar_y_congelado() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let b = expendio(&alm, &admin, "SNB").await;
    let prod = producto_pieza_externo(&alm, &admin, "huevo", "7501000111114").await;

    // Congelado sin precio de menudeo.
    assert!(alm.esta_congelado(&admin, prod, a).await.unwrap());

    // Precio de menudeo independiente por sucursal.
    alm.fijar_precio(&admin, prod, a, Nivel::Menudeo, Centavos::de_pesos(40, 0))
        .await
        .unwrap();
    assert!(!alm.esta_congelado(&admin, prod, a).await.unwrap()); // descongela al fijar menudeo
    assert!(alm.esta_congelado(&admin, prod, b).await.unwrap()); // B sigue sin precio

    // Copiar A → B siembra los niveles de B sin tocar A.
    let copiadas = alm.copiar_precios(&admin, a, b, &[prod]).await.unwrap();
    assert_eq!(copiadas, 1);
    assert!(!alm.esta_congelado(&admin, prod, b).await.unwrap());

    // Editar B es independiente de A.
    alm.fijar_precio(&admin, prod, b, Nivel::Menudeo, Centavos::de_pesos(35, 0))
        .await
        .unwrap();
    assert!(alm.vendible(&admin, prod, a, Nivel::Menudeo).await.unwrap());
}

// -------- 8.4 Ciclo de aprobación --------

#[tokio::test]
async fn ciclo_de_aprobacion() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let cajero = cajero_en(&alm, &admin, "caja", a).await;
    let sesion = alm
        .abrir_sesion(&cajero, a, Centavos::de_pesos(200, 0))
        .await
        .unwrap();

    // Gasto nace pendiente.
    let g = alm
        .registrar_gasto(
            &cajero,
            a,
            sesion.id,
            Centavos::de_pesos(2000, 0),
            "publicidad",
        )
        .await
        .unwrap();
    assert_eq!(
        alm.aprobacion_de(&cajero, g.id).await.unwrap().estado,
        domain::aprobacion::EstadoAprobacion::Pendiente
    );

    // El cajero NO autoriza (no porta `autorizar_gasto`).
    assert!(alm.aprobar_gasto(&cajero, g.id).await.is_err());

    // El admin sí; luego no se re-resuelve (terminal).
    alm.aprobar_gasto(&admin, g.id).await.unwrap();
    assert!(alm.aprobacion_de(&admin, g.id).await.unwrap().autorizado());
    assert!(alm.rechazar_gasto(&admin, g.id, "tarde").await.is_err());

    // Rechazo exige motivo.
    let g2 = alm
        .registrar_gasto(&cajero, a, sesion.id, Centavos::de_pesos(300, 0), "dudoso")
        .await
        .unwrap();
    assert!(alm.rechazar_gasto(&admin, g2.id, "   ").await.is_err());
    alm.rechazar_gasto(&admin, g2.id, "fuera de política")
        .await
        .unwrap();

    // Cancelación: el cajero no cancela lo suyo; el admin sí, con motivo.
    let g3 = alm
        .registrar_gasto(&cajero, a, sesion.id, Centavos::de_pesos(100, 0), "error")
        .await
        .unwrap();
    assert!(
        alm.cancelar_gasto(&cajero, g3.id, "me equivoqué")
            .await
            .is_err()
    );
    alm.cancelar_gasto(&admin, g3.id, "gasto por error")
        .await
        .unwrap();
    assert_eq!(
        alm.aprobacion_de(&admin, g3.id).await.unwrap().estado,
        domain::aprobacion::EstadoAprobacion::Cancelada
    );

    // El gasto cancelado CONSERVA su folio y el número no se recicla (D18).
    let gastos = alm.listar_gastos(&admin, a).await.unwrap();
    let fila_g3 = gastos.iter().find(|g| g.id == g3.id).unwrap();
    assert_eq!(fila_g3.folio.get(), "SNAG3");
    let g4 = alm
        .registrar_gasto(&cajero, a, sesion.id, Centavos::de_pesos(10, 0), "nuevo")
        .await
        .unwrap();
    assert_eq!(
        g4.folio.get(),
        "SNAG4",
        "el folio del cancelado no se reutiliza"
    );
}

// -------- 8.5 Corte de caja --------

#[tokio::test]
async fn corte_solo_autorizados_ciego_e_inmutable() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let cajero = cajero_en(&alm, &admin, "caja", a).await;
    let sesion = alm
        .abrir_sesion(&cajero, a, Centavos::de_pesos(500, 0))
        .await
        .unwrap();

    // Un gasto autorizado (1200) y uno no autorizado (300, pendiente).
    let autorizado = alm
        .registrar_gasto(
            &cajero,
            a,
            sesion.id,
            Centavos::de_pesos(1200, 0),
            "insumos",
        )
        .await
        .unwrap();
    alm.aprobar_gasto(&admin, autorizado.id).await.unwrap();
    alm.registrar_gasto(
        &cajero,
        a,
        sesion.id,
        Centavos::de_pesos(300, 0),
        "sin autorizar",
    )
    .await
    .unwrap();

    // No se abre otra sesión con esta abierta (secuencial).
    assert!(alm.abrir_sesion(&cajero, a, Centavos::CERO).await.is_err());

    // Cierre a ciegas: el cajero captura contado; no ve conciliación.
    let ventas = ResumenVentas {
        efectivo: Centavos::de_pesos(8000, 0),
        ..Default::default()
    };
    alm.cerrar_sesion(&cajero, sesion.id, Centavos::de_pesos(7000, 0), ventas)
        .await
        .unwrap();
    assert!(
        alm.conciliacion(&cajero, sesion.id).await.is_err(),
        "el cajero no ve la conciliación"
    );

    // El admin ve el corte: esperado = 500 + 8000 - 1200 = 7300; faltante = 300 (el no autorizado).
    let corte = alm.conciliacion(&admin, sesion.id).await.unwrap();
    assert_eq!(corte.esperado, Centavos::de_pesos(7300, 0));
    assert_eq!(corte.gastos_autorizados, Centavos::de_pesos(1200, 0));
    assert_eq!(corte.diferencia, Centavos::de_pesos(-300, 0));
    assert!(corte.es_faltante());

    // Corte cerrado inmutable.
    assert!(
        alm.cerrar_sesion(&cajero, sesion.id, Centavos::CERO, ResumenVentas::default())
            .await
            .is_err()
    );

    // Cerrada la anterior, sí abre la siguiente (día siguiente).
    assert!(
        alm.abrir_sesion(&cajero, a, Centavos::de_pesos(300, 0))
            .await
            .is_ok()
    );
}

// -------- 8.6 Auditoría, borrado lógico y atribución al sistema --------

#[tokio::test]
async fn auditoria_atribuye_y_no_borra() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let prod = producto_pieza_externo(&alm, &admin, "huevo", "7501000111114").await;

    // Un cambio de precio queda atribuido con antes→después.
    alm.fijar_precio(&admin, prod, a, Nivel::Menudeo, Centavos::de_pesos(40, 0))
        .await
        .unwrap();
    alm.fijar_precio(&admin, prod, a, Nivel::Menudeo, Centavos::de_pesos(35, 0))
        .await
        .unwrap();
    let entradas = alm.bitacora_de(&admin, "precio", prod).await.unwrap();
    let modif = entradas
        .iter()
        .find(|e| e.operacion == "Modificar")
        .expect("hay una modificación");
    assert_eq!(modif.antes.as_deref(), Some("40.00"));
    assert_eq!(modif.despues.as_deref(), Some("35.00"));
    assert!(modif.actor.starts_with("usuario:"));

    // El bootstrap (sin usuario) queda atribuido al `sistema`.
    let suc = alm
        .sucursal_por_codigo(&admin, "MAT")
        .await
        .unwrap()
        .unwrap();
    let boot = alm.bitacora_de(&admin, "sucursal", suc.id).await.unwrap();
    assert!(
        boot.iter()
            .any(|e| e.actor == "sistema" && e.operacion == "Crear")
    );

    // Borrado lógico: el producto se desactiva pero NO se borra físicamente.
    alm.desactivar_producto(&admin, prod).await.unwrap();
    let sigue = alm
        .producto_por_codigo(&admin, "7501000111114")
        .await
        .unwrap();
    assert!(
        sigue.is_some(),
        "el producto sigue existiendo (borrado lógico)"
    );
    assert!(!sigue.unwrap().activo);
}

// -------- 8.7 Multi-sesión concurrente --------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multi_sesion_concurrente() {
    let (alm, admin) = con_admin().await;
    // Prepara N sucursales, cada una con su cajero.
    let mut sucursales = Vec::new();
    for i in 0..5u32 {
        let cod = format!("S{i:02}");
        let suc = expendio(&alm, &admin, &cod).await;
        let cajero = cajero_en(&alm, &admin, &format!("caja{i}"), suc).await;
        sucursales.push((suc, cajero));
    }

    // Todas operan a la vez, sin interferirse.
    let mut tareas = Vec::new();
    for (suc, cajero) in sucursales {
        let alm = alm.clone();
        tareas.push(tokio::spawn(async move {
            let s = alm
                .abrir_sesion(&cajero, suc, Centavos::de_pesos(100, 0))
                .await?;
            alm.registrar_gasto(&cajero, suc, s.id, Centavos::de_pesos(10, 0), "concurrente")
                .await?;
            storage::Resultado::Ok(suc)
        }));
    }
    for t in tareas {
        let suc = t.await.unwrap().expect("cada sesión opera sin error");
        assert_eq!(alm.listar_gastos(&admin, suc).await.unwrap().len(), 1);
    }
}

// -------- 8.8 Identificadores generados: folio y barcode --------

#[tokio::test]
async fn folios_secuenciales_por_sucursal_y_tipo() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let b = expendio(&alm, &admin, "SNB").await;
    let ca = cajero_en(&alm, &admin, "ca", a).await;
    let cb = cajero_en(&alm, &admin, "cb", b).await;
    let sa = alm.abrir_sesion(&ca, a, Centavos::CERO).await.unwrap();
    let sb = alm.abrir_sesion(&cb, b, Centavos::CERO).await.unwrap();

    let g1 = alm
        .registrar_gasto(&ca, a, sa.id, Centavos::de_pesos(1, 0), "x")
        .await
        .unwrap();
    let g2 = alm
        .registrar_gasto(&ca, a, sa.id, Centavos::de_pesos(1, 0), "y")
        .await
        .unwrap();
    let g_b = alm
        .registrar_gasto(&cb, b, sb.id, Centavos::de_pesos(1, 0), "z")
        .await
        .unwrap();

    // Secuencial por (sucursal, tipo), independiente entre sucursales.
    assert_eq!(g1.folio.get(), "SNAG1");
    assert_eq!(g2.folio.get(), "SNAG2");
    assert_eq!(g_b.folio.get(), "SNBG1");
}

#[tokio::test]
async fn barcodes_matriz_validos_unicos_y_gtin_sin_colision() {
    let (alm, admin) = con_admin().await;

    // Dos productos de matriz reciben EAN-13 válidos y distintos.
    let mut codigos = BTreeSet::new();
    for nombre in ["queso 500g", "salmón 500g"] {
        let p = alm
            .crear_producto(
                &admin,
                BorradorProducto {
                    nombre: nombre.into(),
                    tipo: TipoProducto::Pieza,
                    origen: OrigenProducto::Matriz,
                    codigo: AltaCodigo::GenerarMatriz,
                    peso_empaque: Some(domain::unidades::Gramos::new(500)),
                },
            )
            .await
            .unwrap();
        let ean = p.codigo.unwrap();
        let texto = ean.ean().get().to_string();
        assert!(Ean13::parse(&texto).is_ok(), "EAN-13 decodable");
        assert!(codigos.insert(texto), "barcodes únicos entre productos");
    }

    // Un GTIN externo no puede duplicarse.
    producto_pieza_externo(&alm, &admin, "Nescafé", "7501059224827").await;
    let dup = alm
        .crear_producto(
            &admin,
            BorradorProducto {
                nombre: "otro".into(),
                tipo: TipoProducto::Pieza,
                origen: OrigenProducto::Externo,
                codigo: AltaCodigo::Externo(Ean13::parse("7501059224827").unwrap()),
                peso_empaque: None,
            },
        )
        .await;
    assert!(dup.is_err(), "GTIN duplicado rechazado");
}

// -------- 8.5 (D19) Resolución tardía: el corte cerrado no se reescribe --------

#[tokio::test]
async fn resolucion_tardia_no_reescribe_el_corte() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let cajero = cajero_en(&alm, &admin, "caja", a).await;
    let s1 = alm.abrir_sesion(&cajero, a, Centavos::CERO).await.unwrap();
    let g = alm
        .registrar_gasto(&cajero, a, s1.id, Centavos::de_pesos(300, 0), "tardío")
        .await
        .unwrap();

    // Cierra con el gasto aún pendiente → faltante de 300.
    let ventas = ResumenVentas {
        efectivo: Centavos::de_pesos(1000, 0),
        ..Default::default()
    };
    alm.cerrar_sesion(&cajero, s1.id, Centavos::de_pesos(700, 0), ventas)
        .await
        .unwrap();
    let antes = alm.conciliacion(&admin, s1.id).await.unwrap();
    assert_eq!(antes.esperado, Centavos::de_pesos(1000, 0));
    assert_eq!(antes.diferencia, Centavos::de_pesos(-300, 0));

    // Se autoriza DESPUÉS del cierre (conciliación posterior, permitida).
    alm.aprobar_gasto(&admin, g.id).await.unwrap();

    // El corte cerrado conserva sus cifras EXACTAS (foto histórica)…
    let despues = alm.conciliacion(&admin, s1.id).await.unwrap();
    assert_eq!(despues.esperado, antes.esperado);
    assert_eq!(despues.contado, antes.contado);
    assert_eq!(despues.diferencia, antes.diferencia);
    assert_eq!(despues.gastos_autorizados, antes.gastos_autorizados);

    // …y la resolución queda ligada al gasto con su propia fecha.
    let ap = alm.aprobacion_de(&admin, g.id).await.unwrap();
    assert!(ap.autorizado());
    assert!(
        ap.resuelto_en.is_some(),
        "la resolución se sella con su fecha"
    );

    // El gasto NO salta al corte del día de su resolución (pertenece a s1).
    let s2 = alm.abrir_sesion(&cajero, a, Centavos::CERO).await.unwrap();
    alm.cerrar_sesion(&cajero, s2.id, Centavos::CERO, ResumenVentas::default())
        .await
        .unwrap();
    let corte2 = alm.conciliacion(&admin, s2.id).await.unwrap();
    assert_eq!(corte2.gastos_autorizados, Centavos::CERO);
}

// -------- 8.6 La bitácora es inmutable también desde SQL --------

#[tokio::test]
async fn bitacora_inmutable_desde_sql() {
    let url = url_temporal();
    let alm = Almacen::abrir(&url).await.unwrap();
    alm.bootstrap("MAT", "admin", "clave123").await.unwrap();

    // Conexión cruda, por fuera de la aplicación: los triggers abortan.
    let cruda = sqlx::SqlitePool::connect(&url).await.unwrap();
    let update = sqlx::query("UPDATE bitacora SET actor = 'intruso'")
        .execute(&cruda)
        .await;
    assert!(update.is_err(), "UPDATE sobre la bitácora debe abortar");
    let delete = sqlx::query("DELETE FROM bitacora").execute(&cruda).await;
    assert!(delete.is_err(), "DELETE sobre la bitácora debe abortar");
}

// -------- 3.1 Matriz única y zona horaria por defecto --------

#[tokio::test]
async fn matriz_unica_y_zona_default() {
    let (alm, admin) = con_admin().await;

    // No se registra una segunda matriz.
    let segunda = alm
        .crear_sucursal(
            &admin,
            TipoSucursal::Matriz,
            CodigoSucursal::nueva("MT2").unwrap(),
            "Matriz 2",
            ZonaHoraria::default(),
        )
        .await;
    assert!(segunda.is_err(), "a lo sumo una sucursal matriz");

    // La zona por defecto es America/Mexico_City.
    let mat = alm
        .sucursal_por_codigo(&admin, "MAT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mat.zona.iana(), "America/Mexico_City");
}

// -------- 3.8/4.5 La gestión exige sus permisos `gestionar_*` --------

#[tokio::test]
async fn gestion_requiere_sus_permisos() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let cajero = cajero_en(&alm, &admin, "caja", a).await; // sin gestionar_*

    assert!(
        alm.crear_sucursal(
            &cajero,
            TipoSucursal::Expendio,
            CodigoSucursal::nueva("XXX").unwrap(),
            "pirata",
            ZonaHoraria::default(),
        )
        .await
        .is_err()
    );
    assert!(
        alm.crear_rol(&cajero, "pirata", &[Permiso::Vender])
            .await
            .is_err()
    );
    assert!(
        alm.crear_usuario(&cajero, "pirata", "clave123", &[], Alcance::de([a]))
            .await
            .is_err()
    );
    assert!(
        alm.crear_producto(
            &cajero,
            BorradorProducto {
                nombre: "pirata".into(),
                tipo: TipoProducto::Pieza,
                origen: OrigenProducto::Externo,
                codigo: AltaCodigo::Externo(Ean13::parse("7501059224827").unwrap()),
                peso_empaque: None,
            },
        )
        .await
        .is_err()
    );
}

// -------- Alcance multi-sucursal: opera en las suyas, jamás en una tercera --------

#[tokio::test]
async fn alcance_multisucursal() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let b = expendio(&alm, &admin, "SNB").await;
    let c = expendio(&alm, &admin, "SNC").await;
    let rol = alm
        .crear_rol(
            &admin,
            "multi",
            &[Permiso::RegistrarGasto, Permiso::OperarCaja],
        )
        .await
        .unwrap();
    alm.crear_usuario(&admin, "dua", "clave123", &[rol.id], Alcance::de([a, b]))
        .await
        .unwrap();
    let dua = alm.autenticar("dua", "clave123").await.unwrap();

    assert!(alm.listar_gastos(&dua, a).await.is_ok());
    assert!(alm.listar_gastos(&dua, b).await.is_ok());
    assert!(
        alm.listar_gastos(&dua, c).await.is_err(),
        "jamás una tercera"
    );
}

// -------- Renombrar sucursal: el código y los folios no se alteran --------

#[tokio::test]
async fn renombrar_sucursal_no_altera_codigo_ni_folios() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let cajero = cajero_en(&alm, &admin, "caja", a).await;
    let s = alm.abrir_sesion(&cajero, a, Centavos::CERO).await.unwrap();
    let g1 = alm
        .registrar_gasto(&cajero, a, s.id, Centavos::de_pesos(1, 0), "x")
        .await
        .unwrap();
    assert_eq!(g1.folio.get(), "SNAG1");

    alm.renombrar_sucursal(&admin, a, "San Andrés Renovado")
        .await
        .unwrap();
    let suc = alm
        .sucursal_por_codigo(&admin, "SNA")
        .await
        .unwrap()
        .expect("el código no cambió");
    assert_eq!(suc.nombre, "San Andrés Renovado");
    assert_eq!(suc.codigo.get(), "SNA");

    // La secuencia de folios sigue con el mismo prefijo.
    let g2 = alm
        .registrar_gasto(&cajero, a, s.id, Centavos::de_pesos(1, 0), "y")
        .await
        .unwrap();
    assert_eq!(g2.folio.get(), "SNAG2");
}

// -------- Baja lógica del usuario: no vuelve a entrar (D16) --------

#[tokio::test]
async fn usuario_desactivado_no_entra() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let rol = alm
        .crear_rol(&admin, "temporal", &[Permiso::Vender])
        .await
        .unwrap();
    let u = alm
        .crear_usuario(&admin, "temporal", "clave123", &[rol.id], Alcance::de([a]))
        .await
        .unwrap();
    assert!(alm.autenticar("temporal", "clave123").await.is_ok());

    alm.desactivar_usuario(&admin, u.id).await.unwrap();
    assert!(
        alm.autenticar("temporal", "clave123").await.is_err(),
        "el usuario dado de baja no se autentica"
    );
}

// -------- Aprobaciones pendientes dirigidas al aprobador --------

#[tokio::test]
async fn aprobaciones_pendientes_dentro_del_alcance() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let cajero = cajero_en(&alm, &admin, "caja", a).await;
    let s = alm.abrir_sesion(&cajero, a, Centavos::CERO).await.unwrap();
    let g1 = alm
        .registrar_gasto(&cajero, a, s.id, Centavos::de_pesos(100, 0), "uno")
        .await
        .unwrap();
    alm.registrar_gasto(&cajero, a, s.id, Centavos::de_pesos(200, 0), "dos")
        .await
        .unwrap();

    // El cajero (sin `autorizar_gasto`) no ve la cola.
    assert!(alm.aprobaciones_pendientes(&cajero).await.is_err());

    // El admin ve las dos; al resolver una, queda una.
    assert_eq!(alm.aprobaciones_pendientes(&admin).await.unwrap().len(), 2);
    alm.aprobar_gasto(&admin, g1.id).await.unwrap();
    assert_eq!(alm.aprobaciones_pendientes(&admin).await.unwrap().len(), 1);
}

// -------- El gasto solo se registra contra la sesión abierta de su sucursal --------

#[tokio::test]
async fn gasto_solo_contra_sesion_abierta_de_su_sucursal() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let b = expendio(&alm, &admin, "SNB").await;
    let ca = cajero_en(&alm, &admin, "ca", a).await;
    let cb = cajero_en(&alm, &admin, "cb", b).await;
    let sa = alm.abrir_sesion(&ca, a, Centavos::CERO).await.unwrap();

    // Sesión de OTRA sucursal: rechazado.
    assert!(
        alm.registrar_gasto(&cb, b, sa.id, Centavos::de_pesos(10, 0), "cruzado")
            .await
            .is_err(),
        "la sesión no pertenece a la sucursal del gasto"
    );

    // Sesión CERRADA: rechazado (el registro no es conciliación posterior).
    alm.cerrar_sesion(&ca, sa.id, Centavos::CERO, ResumenVentas::default())
        .await
        .unwrap();
    assert!(
        alm.registrar_gasto(&ca, a, sa.id, Centavos::de_pesos(10, 0), "tarde")
            .await
            .is_err(),
        "no se registran gastos contra un corte cerrado"
    );
}

// -------- 4.4 Producto inactivo no es vendible aunque tenga precios --------

#[tokio::test]
async fn producto_inactivo_no_es_vendible() {
    let (alm, admin) = con_admin().await;
    let a = expendio(&alm, &admin, "SNA").await;
    let prod = producto_pieza_externo(&alm, &admin, "huevo", "7501000111114").await;
    alm.fijar_precio(&admin, prod, a, Nivel::Menudeo, Centavos::de_pesos(40, 0))
        .await
        .unwrap();
    assert!(alm.vendible(&admin, prod, a, Nivel::Menudeo).await.unwrap());

    alm.desactivar_producto(&admin, prod).await.unwrap();
    assert!(
        !alm.vendible(&admin, prod, a, Nivel::Menudeo).await.unwrap(),
        "inactivo ⇒ no vendible, conserve o no sus precios"
    );
}
