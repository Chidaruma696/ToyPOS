## Context

Con `labeling-expiry` archivado, la mercancía ya tiene identidad (etiquetas por-ítem, cajas) y caducidad, y el inventario por sucursal existe desde `inventory-per-branch` — pero nada conecta matriz con los expendios. Las decisiones de negocio de esta capa quedaron cerradas en la exploración del 2026-07-13: matriz **no lleva stock propio** (D34), el stock del sistema **nace con la recepción**, la recepción registra **lo real** con discrepancia notificada, solo direcciones matriz↔expendio, y el alcance es **modelo local sync-ready** (ambos extremos operan sobre la base actual; el transporte físico del payload llega con la capa de sync). El tipo de movimiento `recepcion` quedó reservado esperando esta capa (D23), el folio anticipa el tipo envío (D18) y la alerta "por vencer" está latente hasta que haya etiquetas en expendios (D36).

## Goals / Non-Goals

**Goals:**
- El **envío** como documento citable (folio D18) con ciclo mínimo `preparado → enviado → recibido`, cancelable antes de enviar, sin aprobación (D4).
- Contenido por **identidad**: cajas cerradas y etiquetas sueltas del etiquetado, congeladas en el documento.
- Efectos de inventario correctos bajo D34: la salida se postea solo desde expendios, la entrada solo hacia expendios; saldo y ledger en la misma transacción (D23), no-negatividad (D25).
- **Recepción de lo real**: faltantes extraviados, anotados y notificados a la administración.
- El **viaje de las etiquetas**: lo recibido queda en el destino con su identidad y caducidad — y la alerta "por vencer" se enciende sola.
- Permisos `enviar`/`recibir` sobre el RBAC extensible (D2).

**Non-Goals:**
- Transporte físico del payload y "el envío carga su propio catálogo" (capa de sync futura; aquí no hay multi-nodo).
- Verificación etiqueta-por-etiqueta dentro de una caja recibida (llega con el escaneo del POS; hoy la caja se recibe entera).
- Reversión de movimientos `envio`/`recepcion` (la corrección de saldo ya existe: ajuste absoluto D26).
- Pedidos, surtido automático, reabasto y punto de reorden.
- Clasificación bueno/echado-a-perder de las devoluciones en matriz: sin stock de matriz (D34) no hay saldo que contaminar; el jefe decide mirando. Si algún día matriz lleva stock, se rediscute.

## Decisions

### D38 — El envío es un documento con folio y tres estados, sin aprobación
`Envio` = documento citable con folio propio (`TipoDocumento::Envio`, letra `E`, D18) y estados `preparado → enviado → recibido`, más `cancelado` (solo desde `preparado`; conserva su folio, D18). No hay `pendiente→aprobada|rechazada`: matriz decide, empaca y manda (D4). Los tres momentos quedan con marca de tiempo y actor en la bitácora (D11).
- *Alternativa descartada:* reutilizar la máquina de aprobaciones → es exactamente lo que D4 excluyó; el envío no se solicita, se hace.

### D39 — Un extremo siempre es la matriz
Direcciones válidas: **matriz→expendio** (surtido) y **expendio→matriz** (devolución). Un envío cuyo origen y destino son expendios SHALL rechazarse.
- *Rationale:* el flujo real del negocio es radial (matriz produce y surte; el expendio devuelve); el traspaso lateral complicaría al escritor único del catálogo que viajará con el payload (sync futuro) sin un caso real que lo pida.

### D40 — Efectos de inventario asimétricos (consecuencia directa de D34)
La matriz **jamás postea movimientos** de inventario (no lleva stock propio, D34). Por lo tanto:
- **Surtido (matriz→expendio):** `enviar` no toca stock; `recibir` postea **un movimiento `recepcion` por producto** en el expendio con lo **presente** (etiquetas: suma de pesos en gramos; cajas de pieza: suma de cantidades), motivo = folio del envío.
- **Devolución (expendio→matriz):** `enviar` postea **un movimiento `envio` por producto** (delta negativo) en el expendio — la mercancía sale físicamente en ese momento y la no-negatividad aplica (D25: no puedes devolver más de lo que hay); `recibir` en matriz no postea nada.
Saldo y movimiento van en la **misma transacción** por el camino único existente (D23, DRY).
- *Alternativa descartada:* descontar la devolución al recibir en matriz → el stock del expendio mentiría durante el tránsito, y el asiento pertenece a quien sí lleva stock.

### D41 — Recepción de lo real, por identidad; faltantes extraviados y notificados
`recibir` toma la lista de **presentes** (ids de cajas enteras y de etiquetas sueltas). Lo presente cambia de sucursal; lo declarado y no presente pasa a estado **`extraviada`**: no está en el origen ni en el destino, no cuenta en ningún stock y no genera alertas de caducidad. La discrepancia queda **anotada en el documento** y dispara una notificación tipo `discrepancia_envio` a la **bandeja de la matriz** (administración) vía el mecanismo de `notifications` (D37). Una recepción completa no notifica nada.
- *Granularidad:* la caja se recibe **entera** (llegó o no llegó); contar etiquetas dentro de una caja es del escaneo del POS (futuro). Las etiquetas sueltas sí se confirman una a una — el caso "llegan 38 de 40" es de sueltas.
- *Alternativa descartada:* todo-o-nada → bloquea al expendio con la mercancía en la mano; recibir-completo-y-ajustar-después → disfraza la merma en tránsito de error de conteo.

### D42 — El contenido se congela al preparar; cancelar lo libera
Al `preparar`, las cajas y etiquetas sueltas quedan **ligadas al envío** (columna `envio_id`): deben ser de la sucursal origen, estar **activas** y no comprometidas en otro envío; una etiqueta dentro de una caja viaja con su caja (no se lista aparte). `cancelar` (solo en `preparado`) desliga el contenido y lo deja disponible.
- *Rationale:* el documento describe mercancía concreta (identidad por etiqueta, D31), no cantidades abstractas; congelar al preparar evita que algo se venda o se re-empaque mientras "está en el envío".

### D43 — La corrección posterior usa el ajuste absoluto, no reversión de envíos [YAGNI]
Los movimientos `envio`/`recepcion` **no se revierten** en esta capa (el toggle D27 sigue limitado a `ajuste`). Un error de conteo en la recepción se corrige con el **ajuste absoluto con motivo** (D26), que ya existe y queda auditado; mercancía mal enviada regresa con un **envío de vuelta** (la devolución ya es parte de esta capa).
- *Rationale:* revertir una recepción exigiría des-viajar etiquetas (¿a dónde, si el camión ya se fue?); el mundo físico se corrige con más mundo físico, y el saldo con la herramienta de conteo existente.

### D44 — Solo modelo local sync-ready; el payload viaja con la capa de sync
Ambos extremos operan sobre la base actual (hoy no hay multi-nodo); el esquema nace sync-ready (D12). El **transporte físico** del envío —y el "envío carga su propio catálogo/etiquetas" para sembrar el nodo receptor sin red— se realiza en la capa de **sync**, que serializará este mismo documento.
- *Alternativa descartada:* payload exportable (USB/red local) ya → más superficie de errores en esta capa sin nodo receptor real que lo consuma todavía.

## Risks / Trade-offs

- **Doble efecto al recibir (mover etiquetas + postear stock) se desincroniza** → *Mitigación:* una sola transacción hace ambas cosas; prueba de integración verifica `existencia == suma de etiquetas/cajas presentes` tras recibir.
- **Contenido comprometido que se vende/reempaca mientras el envío está preparado** → *Mitigación:* D42 liga el contenido al documento; cerrar caja y preparar envío validan "sin envío previo"; las capas futuras (POS) deberán respetar `envio_id` ocupado.
- **La caja se recibe entera y esconde faltantes internos** → *Aceptado y explícito* (D41): la granularidad fina llega con el escaneo del POS; mientras tanto la merma interna aflora en la venta o el conteo (ajuste D26).
- **Extraviada como estado terminal silencioso** → mercancía perdida que nadie revisa. *Mitigación:* la discrepancia siempre se anota en el documento y se notifica a la administración; el documento conserva la lista completa (presentes y faltantes) para revisión.
- **`envio_id` en `etiqueta`/`caja_etiquetado` acopla etiquetado con envíos** → *Aceptado:* es el mismo patrón que `caja_id` (una columna de pertenencia), más simple que tablas de líneas; el documento se reconstruye por consulta.

## Migration Plan

Capa aditiva; sin datos que migrar. Tabla nueva `envio` (idempotente) y columnas aditivas `envio_id` en `etiqueta` y `caja_etiquetado` (mismo mecanismo `COLUMNAS_ADITIVAS` que `vida_util`). Enums extensibles crecen: `TipoDocumento::Envio`, `TipoMovimiento::Envio`, `EstadoEtiqueta::Extraviada`, `TipoNotificacion::DiscrepanciaEnvio`, permisos `Enviar`/`Recibir` (los CHECK de SQL correspondientes se amplían). Rollback = no crear tabla/columnas ni exponer el trait; nada existente depende de ellas.

## Open Questions

Ninguna: las decisiones de negocio (stock de matriz, discrepancias, direcciones, alcance de transporte) quedaron resueltas con el usuario en la exploración del 2026-07-13.
