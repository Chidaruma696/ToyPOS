## Context

La fundación (`foundation-core`, archivada) dejó el catálogo global de productos, el RBAC con alcance como aislamiento (D3), unidades enteras (D6), la bitácora transversal (D11), el esquema sync-ready (D12), el principio de reversión (D15) y la meta de ruta caliente instantánea (D17). Falta el **inventario**: cuántas unidades/gramos de cada producto hay en cada sucursal.

Este cambio agrega esa capa sobre el mismo workspace (`domain` puro + `storage` SQLite) y bajo los mismos principios (KISS/DRY/YAGNI, Kanso para las caras futuras). No construye ninguna de las tres caras ni sus fuentes de movimiento (recepción, POS): asienta la existencia y su ledger, listos para que esas capas posteen.

Las decisiones continúan el log global de la fundación (que terminó en D22); las referencias `D3`, `D6`, `D11`, `D15`, `D17`, `D18` son de la fundación.

## Goals / Non-Goals

**Goals:**
- **Existencia por `producto × sucursal`** correcta y tipada, en la unidad natural del producto (D6).
- Un **ledger de movimientos** append-only, atribuible (D11) y reversible (D15), como verdad del inventario.
- **Lectura O(1)** del saldo en la ruta caliente (D17), sin recorrer el ledger.
- **No negatividad** garantizada por diseño.
- Aislamiento por alcance (D3) reutilizando el punto único de acceso a datos.
- Dejar el enganche listo para que recepción y POS posten movimientos sin reescribir reglas.

**Non-Goals:**
- Recepción con validación de completitud y el postado de movimientos `Recepcion` (capa futura).
- POS, descuento de stock por venta (`Venta`) y la política de "no vender sin stock" (capa futura).
- Reabasto automático, punto de reorden / stock mínimo, y reportes de existencias o más vendidos.
- Costeo/valuación del inventario (costo promedio, PEPS): no se pide hoy (YAGNI).
- Traspaso físico entre sucursales (vive en envío/recepción).
- **Devoluciones** (cliente → tienda, tienda → matriz, o mercancía no vendida directa a matriz): capacidad futura que se apoyará en la **reversión** (D15/D27) y en los movimientos de esta capa. Aquí solo se construye el mecanismo (reversión + ajuste absoluto), no el flujo de devolución.

## Decisions

### D23 — Inventario = ledger de movimientos + saldo materializado
La existencia se modela como un **ledger append-only de movimientos** (la verdad) más un **saldo materializado** por `(producto, sucursal)` que se actualiza en la **misma transacción** que el movimiento. El ledger da historial, atribución y reversión; el saldo da lectura O(1) para la ruta caliente.
- *Por qué ambos:* sumar un ledger creciente en cada lectura de precio/venta violaría D17 (full-scan en hot path); un saldo materializado indexado por la clave caliente lo evita. Es el patrón contable clásico (asientos + saldo), no una duplicación frágil: el saldo se deriva mecánicamente del ledger dentro de la transacción, nunca a mano.
- *No duplica la bitácora (D11):* la bitácora es infraestructura transversal que responde *quién* cambió algo; el movimiento es **dato de dominio** que responde *qué, cuánto y por qué*, y es consumible (reabasto por rotación, reportes de más vendidos). Son registros distintos con preguntas distintas.
- *Alternativa descartada:* solo saldo (pierde el *qué/por qué*, imposibilita reversión y rotación). *Alternativa descartada:* solo ledger sumado en cada lectura (rompe D17).

### D24 — Cantidad tipada por la unidad del producto (D6)
La cantidad de existencia y de cada movimiento se expresa en la **unidad natural del producto**: `pieza` → **unidades enteras**; `peso_variable` → **gramos** (D6). Un tipo `Cantidad` lleva su unidad, de modo que el compilador impide mezclar gramos con unidades o sumar peras con manzanas.
- *Deriva del producto:* la unidad se toma de `Producto::unidad_venta` (ya existente); un movimiento contra un producto valida que su cantidad use la unidad correcta.
- *Alternativa descartada:* un `i64` desnudo interpretado por convención → invita a sumar unidades a gramos sin que nada lo note.

### D25 — El stock no es negativo; el exceso se rechaza
La existencia SHALL ser siempre ≥ 0. Un movimiento de salida (o un ajuste por delta) que dejaría el saldo por debajo de cero se **rechaza** antes de aplicarse; el saldo nunca queda negativo.
- *Rationale:* un inventario negativo es un dato que miente; es preferible rechazar y forzar la corrección explícita.
- *"No vender sin stock" es del POS (futuro):* aquí solo se garantiza la no-negatividad del saldo; si el POS permite o no sobrevender con stock 0 es su decisión, tomada sobre esta base honesta.

### D26 — Un único escritor real hoy: el ajuste manual **absoluto** con motivo
El único origen de movimientos que se construye en esta capa es el **ajuste manual**, con **motivo obligatorio**, protegido por `ajustar_inventario` + alcance. El ajuste **fija la existencia a la cantidad contada** (valor **absoluto**): el usuario captura el número que contó y el sistema deriva el movimiento (`delta = objetivo − saldo`). **No** hay API por delta (KISS): un decremento —merma, o devolución de mercancía no vendida a matriz— se expresa **recontando al nuevo valor**. Los tipos de movimiento `Recepcion` y `Venta` se **definen** en un enum extensible pero **no se implementan** aquí: los postean recepción y POS cuando existan (YAGNI).
- *Por qué absoluto:* es el modelo mental del que está frente al anaquel (alta inicial y conteo físico son "hay N"), y es inequívoco frente a concurrencia ("poner en 48" siempre es 48). El ledger guarda el delta resultante, así que la reversión y el historial funcionan igual.
- *Por qué el enum extensible ya:* deja el tipo del ledger listo para sus consumidores futuros sin migración, sin construir sus flujos ahora.
- *Motivo obligatorio en el ajuste:* el ajuste manual es justo el punto donde suele ocurrir el fraude que motiva D11; exigir motivo + atribución cierra ese hueco.

### D27 — Reversión de movimientos (realiza D15 para inventario)
Un movimiento lleva estado `aplicado ⇄ revertido`. Revertir postea un **movimiento compensatorio** que deshace exactamente su efecto sobre el saldo y **togglea** el estado de la entidad original (no apila copias, no borra). Rehacer vuelve a aplicar. Cada alternancia se audita (D11).
- *Estado limpio, historia intacta:* el saldo vuelve a lo correcto y el ledger conserva la evidencia (movimiento + su compensación), atribuida. Indispensable para el ajuste erróneo del admin.
- *Por qué toggle y no borrado ni copias nuevas:* borrar rompe la auditoría; apilar copias ensucia el sync (D12) y el conteo de rotación. Un toggle + compensación es un append que sincroniza limpio (calca D15).
- *Alcance:* se revierte un **movimiento de ajuste**; la reversión de ventas/recepciones vive con esas capas, sobre este mismo mecanismo.

### D28 — Permisos nuevos, por-sucursal, sobre el RBAC extensible
Se añaden dos permisos atómicos **por-sucursal**: `ajustar_inventario` (escribe) y `ver_inventario` (lee). El catálogo de permisos es extensible por diseño (D2): sumar variantes al enum **no modifica** el mecanismo de `organization-access`. El chequeo es el de siempre: `tiene(permiso) ∧ alcance cubre la sucursal` (D3), de modo que un expendio no ve ni toca el inventario de otro.

### D29 — Los movimientos no llevan folio (no son documentos citables)
Un movimiento de inventario es una **entrada de ledger** identificada por su `uuid`, no un documento que la gente imprima o cite; por eso **no** recibe folio (D18 reserva el folio para documentos/transacciones: notas, cortes, gastos). La **recepción** futura sí será un documento con su folio; el ajuste de hoy no emite documento. Las entidades nacen sync-ready (D12): `uuid`, `updated_at`, y el ledger es append-only por naturaleza (sin borrado físico).

### D30 — Catálogo global, existencia local y perezosa
El producto vive una sola vez (catálogo global gobernado por matriz); la **existencia es por sucursal**. Dar de alta un producto **no** crea filas de existencia en cada sucursal: la existencia se **materializa al primer movimiento**, y su ausencia significa 0. Así se evitan N×M filas vacías y el alta de producto queda desacoplada del inventario.
- *Compone con la estrategia offline (D22/diferida):* el catálogo replicado permite decodificar el barcode en local; la existencia local es de cada nodo. La siembra de existencia por envío es de la capa de recepción, no de aquí.

## Risks / Trade-offs

- **Saldo materializado que se desincroniza del ledger** → *Mitigación:* el saldo se actualiza **exclusivamente** dentro de la misma transacción que inserta el movimiento, por un único camino de código (DRY); nunca se edita a mano. Pruebas que verifican `saldo == suma(movimientos)` tras N operaciones.
- **Condición de carrera en el saldo bajo concurrencia** (dos salidas simultáneas que juntas sobregiran) → *Mitigación:* la lectura-del-saldo, la validación de no-negatividad y la escritura ocurren en una sola transacción serializada por el escritor único de SQLite (foundation: `max_connections=1`); no hay ventana entre leer y escribir.
- **Elegir gramos/unidades por producto complica los movimientos mixtos** → *Mitigación:* `Cantidad` tipada por unidad; un movimiento valida su unidad contra el producto y rechaza la mezcla en compilación/validación.
- **Sobre-modelar (costeo, reorden, tipos de movimiento no usados)** → *Mitigación:* YAGNI estricto — solo ajuste + reversión hoy; costeo y reorden fuera; los tipos futuros son variantes de enum sin flujo.

## Migration Plan

Capa aditiva sobre la fundación archivada; sin datos previos que migrar. El esquema agrega dos tablas nuevas (`existencia`, `movimiento_inventario`) idempotentes (`CREATE TABLE IF NOT EXISTS`), aplicadas al abrir el almacén como el resto del esquema. Rollback = no crear las tablas / no exponer el trait; nada existente depende de ellas.

## Open Questions

Las dos preguntas de esta propuesta quedaron **resueltas** con el usuario:

- **Ajuste absoluto (decidido).** El ajuste fija la existencia a la cantidad contada; sin API por delta (ver D26). El sistema es restrictivo: la existencia jamás es negativa (D25), y una baja (merma/devolución) se expresa recontando al nuevo valor.
- **Punto de reorden diferido al reabasto (decidido).** La configuración de stock mínimo vive con su único consumidor —el cambio de **reabasto**— y allí por `(producto × sucursal)`; esta capa solo provee el saldo y el historial que ese cálculo necesitará.

No quedan preguntas abiertas.
