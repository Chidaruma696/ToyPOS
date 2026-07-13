## Context

La fundación dejó el etiquetado explícitamente diferido: D22 acotó que la **etiqueta de báscula** de `peso_variable` (peso por ítem + discriminador antiduplicado) es *otra clase de código*, per-ítem, distinta del barcode estable de producto; D8 fijó que el precio **nunca** viaja en la etiqueta. El catálogo global (escritor único: matriz) e inventario por sucursal ya existen. Las decisiones de negocio de esta capa vienen cerradas con el usuario: caducidad nominal de 9 meses default, etiqueta física de 3 elementos, alerta "por vencer" a 5 días, y un mecanismo nuevo de notificaciones. La capa UI (Tauri/Vue) aún no existe; el diseño Kanso de la pantalla de etiquetado ya está decidido y se registra aquí para la cara futura.

## Goals / Non-Goals

**Goals:**
- **Identidad por etiqueta** para `peso_variable`: cada pesada = una etiqueta con barcode per-ítem (peso + discriminador), persistida y consultable — el enganche que el POS (decode) y el envío (viaje de etiquetas) usarán después.
- **Caducidad nominal** calculada al etiquetar (`fecha_etiquetado + vida_util del producto`), impresa en la etiqueta; `fecha_etiquetado` interna, jamás impresa; la caducidad **jamás bloquea** operación alguna.
- **Vida útil por producto** en el catálogo (default 9 meses), editable sin reetiquetar lo ya emitido.
- **Cajas** como agrupación mínima de lo etiquetado (unidad natural del traslado futuro).
- **Notificaciones sistema→usuarios** mínimas (bandeja por sucursal) y la alerta **"por vencer"** como primer productor, lista para activarse cuando el envío ponga etiquetas en expendios.
- Permisos nuevos sobre el RBAC extensible (D2/D28): `etiquetar`, `ver_notificaciones`.

**Non-Goals:**
- Impresión física (driver/formato de impresora) y la pantalla de etiquetado (la capa UI no existe aún; su diseño queda registrado abajo).
- Movimiento de inventario por producción/etiquetado (el stock sigue gobernado por el ajuste D26 hasta envío/recepción).
- Envío/traspaso entre sucursales y el viaje de etiquetas/cajas (capa siguiente); venta y el estado "vendida" real (POS futuro).
- Notificaciones externas (email/push), suscripciones, severidades o un bus de eventos genérico.
- Decode del barcode per-ítem en el POS (capa POS; aquí solo se garantiza que la etiqueta persistida alcanza para resolverlo offline).

## Decisions

### D31 — Etiqueta por-ítem: el barcode carga peso + discriminador; el registro carga el resto
El barcode per-ítem es un EAN-13 con **prefijo interno propio** (`21`, distinto del `200` de producto), cuerpo = **discriminador** (5 dígitos) + **peso en gramos** (5 dígitos, hasta 99 999 g por pesada) y verificador. El **producto NO viaja en el barcode**: la etiqueta persistida (`id, discriminador, producto, peso, fecha_etiquetado, caducidad, sucursal, estado`) es la identidad; decodificar = lookup local offline contra las etiquetas replicadas (la fila viajará con el envío, igual que el catálogo).
- *Rationale:* 13 dígitos no alcanzan para producto+peso+discriminador; la identidad-por-registro compone con el offline-first (lookup local) y con D22.
- *Antiduplicado:* dos pesadas idénticas del mismo producto reciben discriminadores distintos → etiquetas distintas. El discriminador sale de una **secuencia persistida** módulo 10⁵; si el candidato colisiona con una etiqueta **activa**, se avanza y reintenta.
- *Alternativa descartada:* codificar el producto en el barcode (price/product-embedded) → no cabe con peso+discriminador y ata el código al catálogo; el registro local ya resuelve.

### D32 — La caducidad es un dato congelado de la etiqueta, nominal e informativa
`caducidad = fecha_etiquetado + vida_util(producto)`, calculada **al etiquetar** y congelada en la etiqueta: cambiar la vida útil del producto después no reetiqueta lo emitido. Es **nominal** (el producto va congelado y al vacío): se imprime para tranquilidad del cliente (`DD/MM/AAAA`), guía la rotación FIFO, y **jamás bloquea la venta** — ni vencida. La `fecha_etiquetado` se guarda internamente y **nunca se imprime**. "El día" de las fechas se define en la zona horaria de la sucursal (D21).
- *Alternativa descartada:* caducidad como bloqueo de venta → contradice la realidad física (no caduca de verdad) y mete una excepción a la ruta caliente que nadie pidió.

### D33 — Vida útil en meses calendáricos enteros, default 9
`vida_util` es un **entero de meses** (D6: nunca float), default **9**, editable por producto con `gestionar_productos`. La suma es calendárica con clamp de fin de mes (31/may + 9 meses → 28/29 feb).
- *Alternativa descartada:* días fijos (270) → fechas impresas "raras" que no corresponden a lo que un humano espera de "9 meses".

### D34 — Matriz produce sin llevar inventario propio; etiquetar no mueve stock
El acto de etiquetar **registra etiquetas y cajas, no movimientos de inventario**. Decisión de negocio (2026-07-13): **matriz no lleva stock propio** — el negocio hoy no tiene un stock concreto en matriz ni ha pedido llevarlo (YAGNI); matriz produce, empaca y envía. El inventario del sistema es el de los **expendios** y nacerá con la **recepción** del envío (capa siguiente; tipos de movimiento ya reservados, D23). El ajuste absoluto (D26) sigue disponible donde sí se lleva stock.
- *Rationale:* llevar un saldo de matriz que nadie cuenta físicamente sería inventar un problema no pedido — p. ej. responder "¿cuánto pollo hay para enviar?" con un número que el congelador desmiente. El jefe decide mirando; el sistema registra lo que de verdad viaja (etiquetas y envíos, con identidad por-ítem).
- *Puerta abierta:* si el negocio algún día pide stock de producción en matriz, el enum de movimientos es extensible — un tipo `produccion` posteado al etiquetar — sin tocar este diseño.
- *Alternativa descartada:* etiquetar postea entrada y enviar postea salida en matriz → doble asiento para un saldo que nadie audita físicamente; se reconsiderará solo si aparece el pedido real.

### D35 — `pieza` no lleva etiqueta por-ítem: caja con cantidad
Los `pieza` usan su **barcode estable de producto** (D22, uno por SKU): etiquetarlos = registrar la **cantidad en la caja**; no se pesa nada. Las **cajas** agrupan lo etiquetado — etiquetas por-ítem para `peso_variable`, cantidad para `pieza`— y son la unidad natural del traslado futuro. La **caducidad impresa aplica solo a lo producido por matriz** (tiene `fecha_etiquetado` propia); los externos conservan la etiqueta de su fábrica y su caja sirve para conteo/traslado.
- *Alternativa descartada:* etiqueta por-ítem también para `pieza` → no aporta identidad útil (todas las unidades son idénticas y ya portan su GTIN) y duplica impresión.

### D36 — Alerta "por vencer": barrido del sistema, idempotente, agrupado por producto×sucursal
Un barrido del **sistema** (actor `sistema`, D11) evalúa etiquetas y cajas **activas** en sucursales tipo **expendio** cuya caducidad esté a **≤ 5 días** (en la zona de la sucursal) y emite **una notificación por grupo** `(producto, sucursal)` — "40 pzas tripa de pollo por vencer en Expendio X" — a **dos bandejas**: la sucursal afectada y la matriz (administración). No re-notifica el mismo grupo mientras la notificación previa siga vigente (idempotencia). El cajero no hace nada: es automático. Hoy queda **latente** (no hay etiquetas en expendios hasta el envío); el mecanismo se prueba sembrando etiquetas en expendio directamente.
- *Alternativa descartada:* notificación por etiqueta individual → ruido inútil (cientos de avisos); la acción operativa es por lote de producto.

### D37 — Notificaciones: bandeja mínima por sucursal, no un bus [KISS, espejo de D4]
Una notificación es `{tipo, mensaje, sucursal_destino, creado, leida_en}`: el sistema las **emite**, la bandeja de cada sucursal las **lista**, y se **marcan leídas por sucursal** (son avisos operativos del local, no correo personal). Leer/marcar exige el permiso por-sucursal `ver_notificaciones`. Sin suscripciones, severidades ni canales externos; el tipo es un enum extensible — igual que la aprobación quedó concreta para gastos (D4), esto queda concreto para avisos del sistema.
- *Alternativa descartada:* notificación por-usuario con estado de lectura individual → duplica filas y estados para un aviso que la sucursal atiende como equipo.

### Diseño de la cara de etiquetado (Kanso — decidido, diferido a la capa UI)
Registrado aquí para que la futura cara lo realice sin rediscusión:
- **Una sola pantalla** que se **reordena según `producto.tipo`** (el tipo del producto elegido manda; sin checkbox "código de fábrica" — deriva del catálogo, DRY).
- *`peso_variable`*: una sola **lista de pesadas** (báscula o manual); el batch "N iguales × peso" es un **quick-add** que inyecta filas (no columna paralela); resumen vivo (N pesadas, kg total).
- *`pieza`*: sin pesar; solo **cantidad en la caja**; usa su GTIN; solo etiqueta de caja.
- **"Imprimir al vuelo"** y **"Caja / Suelta"** son toggles quietos (preferencia recordada / segmented de dos), no modos que dominan la pantalla.
- **Discriminador y caducidad = automáticos** (salen en la etiqueta), **nunca** inputs del formulario.
- **Una sola acción primaria: "Cerrar caja"**. Meta: misma capacidad, ~mitad de ruido visual.

## Risks / Trade-offs

- **Wrap del discriminador (10⁵) colisiona con una etiqueta activa vieja** → *Mitigación:* al generar se verifica contra activas y se reintenta con la siguiente secuencia; 100 000 valores dan margen amplio entre wraps.
- **La alerta queda latente hasta el envío** (no hay etiquetas en expendios todavía) → riesgo de spec sin ejercicio real. *Mitigación:* pruebas de integración siembran etiquetas en un expendio y verifican barrido, agrupación, doble destino e idempotencia.
- **Meses calendáricos y fin de mes** → fechas ambiguas (31 + 9 meses). *Mitigación:* clamp al último día del mes destino, definido en spec y probado en los bordes (fin de mes, año bisiesto).
- **Doble verdad etiquetado vs inventario** → alguien podría leer "lo etiquetado" como stock. *Mitigación:* D34 lo excluye explícitamente (matriz no lleva stock propio); la spec de `labeling` no expone saldos, y el stock del sistema nace con la recepción en el expendio (capa envío).
- **Bandeja por sucursal sin lectura individual** → un usuario podría "perderse" un aviso que otro marcó leído. *Aceptado:* el aviso es operativo del local y el destino doble (sucursal + matriz) ya garantiza dos pares de ojos.

## Migration Plan

Capa aditiva sobre lo archivado; sin datos que migrar. `vida_util` entra con default 9 en productos existentes (columna con `DEFAULT`); tablas nuevas (`etiqueta`, `caja_etiquetado`, `notificacion`, secuencia del discriminador) idempotentes al abrir el almacén, como el resto del esquema. Rollback = no crear tablas / no exponer traits; nada existente depende de ellas.

## Open Questions

- **Formato impreso de la caducidad**: `DD/MM/AAAA` quedó como default acordado, marcado "por confirmar" — se confirma (o ajusta) cuando exista la impresión física real; no afecta al modelo (la etiqueta guarda la fecha, no el string).
