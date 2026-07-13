# labeling Specification

## Purpose

Define el **etiquetado de producción** de ToyPOS: etiquetas **por-ítem** con identidad propia para `peso_variable` (EAN-13 per-ítem con peso + discriminador antiduplicado, resuelto por lookup local), cantidad por **caja** para `pieza`, el contenido físico de la etiqueta (exactamente 3 elementos: barcode, nombre y caducidad `DD/MM/AAAA` — sin precio ni fecha de etiquetado), la **caducidad nominal** congelada al etiquetar (`fecha_etiquetado + vida_util`, jamás bloquea la venta) y la **alerta automática "por vencer"** (5 días antes, solo expendios, agrupada por producto, a la sucursal y a la administración). Etiquetar **no mueve inventario**: matriz produce sin llevar stock propio; el stock del sistema nace con la recepción en los expendios (capacidad `shipments`).

## Requirements

### Requirement: Etiqueta por-ítem con identidad propia para peso variable
Al etiquetar un producto `peso_variable`, cada pesada SHALL producir una **etiqueta** con identidad propia: un EAN-13 per-ítem con **prefijo interno reservado** (distinto del prefijo de barcode estable de producto), cuyo cuerpo codifica el **peso en gramos** y un **discriminador antiduplicado**, con dígito verificador correcto. El sistema SHALL persistir la etiqueta con producto, peso, fecha de etiquetado, caducidad, sucursal y estado, de modo que el barcode se resuelva por **lookup local** (el barcode no carga el producto). Dos pesadas del mismo producto con el mismo peso SHALL recibir discriminadores distintos. Una pesada cuyo peso no cabe en el espacio del código (mayor a 99 999 g) o no es positiva SHALL rechazarse. Etiquetar SHALL exigir el permiso `etiquetar` y un alcance que cubra la sucursal.

#### Scenario: Pesadas idénticas generan etiquetas distintas
- **WHEN** se etiquetan dos pesadas del mismo producto con exactamente el mismo peso
- **THEN** cada una recibe su propia etiqueta con discriminador distinto y barcodes que no coinciden

#### Scenario: El barcode per-ítem es un EAN-13 válido con el peso recuperable
- **WHEN** se genera la etiqueta de una pesada de 1 250 g
- **THEN** el barcode tiene 13 dígitos, porta el prefijo per-ítem, su verificador es correcto y los dígitos de peso codifican 1250

#### Scenario: La etiqueta se resuelve por lookup local
- **WHEN** se consulta una etiqueta persistida por su código
- **THEN** el sistema devuelve su producto, peso, caducidad y estado sin depender de datos embebidos en el barcode

#### Scenario: Etiquetar sin permiso se rechaza
- **WHEN** un usuario sin `etiquetar` (o cuyo alcance no cubre la sucursal) intenta etiquetar pesadas
- **THEN** el sistema rechaza la operación

#### Scenario: Pesada fuera de rango se rechaza
- **WHEN** se intenta etiquetar una pesada de 0 g o de más de 99 999 g
- **THEN** el sistema rechaza la pesada sin emitir etiqueta

### Requirement: Contenido físico de la etiqueta: exactamente tres elementos
El contenido imprimible de una etiqueta SHALL constar de **exactamente tres elementos**, de arriba hacia abajo: (1) el **barcode**, (2) el **nombre del producto** hasta dos líneas, y (3) la **caducidad** en formato `DD/MM/AAAA`. El contenido SHALL NO incluir el precio ni la fecha de etiquetado.

#### Scenario: La etiqueta no expone precio ni fecha de etiquetado
- **WHEN** se compone el contenido imprimible de una etiqueta
- **THEN** contiene solo barcode, nombre (≤ 2 líneas) y caducidad `DD/MM/AAAA`, sin precio y sin fecha de etiquetado

### Requirement: Caducidad nominal calculada y congelada al etiquetar
La caducidad de una etiqueta SHALL calcularse **al etiquetar** como `fecha_etiquetado + vida_util del producto` en **meses calendáricos**, con ajuste al último día del mes destino cuando el día no existe. La `fecha_etiquetado` SHALL guardarse internamente y no formar parte del contenido impreso. La caducidad SHALL quedar **congelada** en la etiqueta: un cambio posterior de la vida útil del producto SHALL NO alterar etiquetas ya emitidas. "El día" SHALL definirse en la zona horaria de la sucursal.

#### Scenario: Caducidad con la vida útil default
- **WHEN** se etiqueta el 13/01 un producto con vida útil de 9 meses
- **THEN** la caducidad de la etiqueta es el 13/10 del mismo año

#### Scenario: Fin de mes se ajusta al último día del mes destino
- **WHEN** se etiqueta el 31/05 un producto con vida útil de 9 meses
- **THEN** la caducidad es el 28/02 (o 29/02 en bisiesto), el último día del mes destino

#### Scenario: Cambiar la vida útil no reetiqueta lo emitido
- **WHEN** se edita la vida útil de un producto después de haber emitido etiquetas
- **THEN** las etiquetas ya emitidas conservan su caducidad original

### Requirement: La caducidad jamás bloquea la operación
La caducidad SHALL ser puramente **informativa**: el sistema SHALL NO impedir vender, mover ni operar una etiqueta por estar próxima a vencer o vencida. Su función es la tranquilidad del cliente y la rotación FIFO.

#### Scenario: Una etiqueta vencida sigue operable
- **WHEN** una etiqueta tiene caducidad anterior a hoy
- **THEN** ninguna operación sobre ella se bloquea por esa causa

### Requirement: Etiquetado de pieza: cantidad por caja, sin etiqueta por-ítem
Un producto `pieza` SHALL etiquetarse **sin pesar y sin etiquetas por-ítem**: usa su barcode estable de producto y solo se registra la **cantidad contenida en la caja**. La caducidad impresa SHALL aplicar únicamente a productos de **origen matriz** (que tienen fecha de etiquetado propia); la caja de un producto externo SHALL NO llevar caducidad impresa (conserva la de su fábrica).

#### Scenario: Pieza registra cantidad por caja
- **WHEN** se etiqueta una caja de un producto `pieza` con 24 unidades
- **THEN** el sistema registra la caja con su producto y cantidad, sin generar etiquetas por-ítem

#### Scenario: Caja de pieza producida por matriz lleva caducidad
- **WHEN** se etiqueta una caja de un `pieza` de origen matriz
- **THEN** el contenido de su etiqueta de caja incluye la caducidad calculada

#### Scenario: Caja de producto externo sin caducidad propia
- **WHEN** se etiqueta una caja de un `pieza` de origen externo
- **THEN** el contenido de su etiqueta de caja no incluye caducidad del sistema

### Requirement: Las cajas agrupan lo etiquetado
El sistema SHALL permitir **cerrar una caja** que agrupa lo etiquetado — las etiquetas por-ítem de `peso_variable`, o la cantidad de `pieza` — dejando la caja como unidad consultable (producto, contenido, fechas) para el traslado futuro. Cerrar una caja SHALL exigir el permiso `etiquetar` y alcance sobre la sucursal.

#### Scenario: Cerrar una caja de pesadas agrupa sus etiquetas
- **WHEN** se cierra una caja con N pesadas etiquetadas de un producto `peso_variable`
- **THEN** la caja queda registrada con sus N etiquetas y su peso total consultable

### Requirement: Etiquetar no altera el inventario
El acto de etiquetar (pesadas, etiquetas o cajas) SHALL NO producir movimientos de inventario ni alterar existencias: matriz produce **sin llevar stock propio**; el inventario del sistema nace con la recepción en los expendios (capacidad `shipments`). El ajuste absoluto sigue disponible donde sí se lleva stock.

#### Scenario: La existencia no cambia al etiquetar
- **WHEN** se etiquetan pesadas y se cierran cajas de un producto en una sucursal
- **THEN** la existencia de ese producto en esa sucursal permanece igual

### Requirement: Alerta automática de stock por vencer
El sistema SHALL detectar **automáticamente** (barrido del sistema, sin acción humana) las etiquetas y cajas **activas** ubicadas en sucursales tipo **expendio** cuya caducidad esté a **5 días o menos** (en la zona horaria de la sucursal), y SHALL emitir **una notificación por grupo** `producto × sucursal` con la cantidad total agrupada, dirigida a **dos bandejas**: la de la sucursal afectada y la de la matriz (administración). El barrido SHALL ser **idempotente**: mientras la notificación de un grupo siga vigente, no se re-emite. El stock en matriz SHALL NO generar esta alerta.

#### Scenario: Alerta agrupada por producto en el expendio
- **WHEN** 40 etiquetas de un producto en un expendio entran en la ventana de 5 días
- **THEN** se emite una sola notificación con el producto, la cantidad agrupada y la sucursal, no cuarenta

#### Scenario: La alerta llega a ambos: sucursal y administración
- **WHEN** el barrido detecta un grupo por vencer en un expendio
- **THEN** la notificación aparece tanto en la bandeja del expendio como en la de la matriz

#### Scenario: El barrido no duplica avisos
- **WHEN** el barrido corre de nuevo y el grupo por vencer sigue en la misma situación
- **THEN** no se emite una segunda notificación para ese grupo

#### Scenario: El stock de matriz no alerta
- **WHEN** hay etiquetas por vencer ubicadas en la matriz
- **THEN** el barrido no emite notificación por ellas

#### Scenario: Fuera de la ventana no hay alerta
- **WHEN** las etiquetas de un producto en un expendio caducan en más de 5 días
- **THEN** el barrido no emite notificación para ese grupo
