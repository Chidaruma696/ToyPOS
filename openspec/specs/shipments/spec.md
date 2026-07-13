# shipments Specification

## Purpose

Define el **envío entre sucursales** de ToyPOS: un documento citable con folio y ciclo `preparado → enviado → recibido` (cancelable antes de enviar), **sin flujo de aprobación** — matriz decide y manda. Un extremo siempre es la matriz: matriz→expendio (surtido) y expendio→matriz (devolución), sin traspasos laterales. El contenido son cajas cerradas y etiquetas sueltas del etiquetado, congeladas en el documento. Los efectos de inventario son **asimétricos** (matriz no lleva stock propio): la devolución postea la salida `envio` al enviar y el surtido postea la entrada `recepcion` al recibir en el expendio — ahí **nace el stock del sistema** y se enciende la alerta "por vencer" de `labeling`. La recepción registra **lo real**: los faltantes quedan extraviados, anotados en el documento y notificados a la administración vía `notifications`. El transporte físico del payload (y sembrar catálogo/etiquetas sin red) pertenece a la capa de sync futura.

## Requirements

### Requirement: El envío es un documento con folio y un extremo en la matriz
El sistema SHALL modelar el **envío** entre sucursales como un documento citable con **folio** propio (secuencial por sucursal origen, tipo envío). Las direcciones válidas SHALL ser **matriz→expendio** (surtido) y **expendio→matriz** (devolución): uno de los dos extremos siempre es la matriz, y origen y destino son distintos. Preparar un envío SHALL exigir el permiso `enviar` y un alcance que cubra la sucursal **origen**. No existe flujo de aprobación: el envío se prepara, se envía y se recibe.

#### Scenario: Surtido de matriz a expendio con folio
- **WHEN** un usuario con `enviar` y alcance sobre la matriz prepara un envío hacia un expendio
- **THEN** el envío queda en estado preparado con un folio citable (p. ej. `MATE1`)

#### Scenario: Traspaso entre expendios se rechaza
- **WHEN** se intenta preparar un envío cuyo origen y destino son expendios
- **THEN** el sistema rechaza la operación

#### Scenario: Preparar sin permiso o fuera de alcance se rechaza
- **WHEN** un usuario sin `enviar` (o cuyo alcance no cubre el origen) intenta preparar un envío
- **THEN** el sistema rechaza la operación

### Requirement: El contenido se congela al preparar y se libera al cancelar
El contenido de un envío SHALL componerse de **cajas cerradas** y **etiquetas sueltas** del etiquetado, pertenecientes a la sucursal **origen**, **activas** y no comprometidas en otro envío; una etiqueta agrupada en una caja viaja con su caja y SHALL NO listarse aparte. Al preparar, el contenido queda **ligado al documento**. Cancelar un envío SHALL permitirse solo en estado `preparado`: libera su contenido y deja el documento como `cancelado`, conservando su folio.

#### Scenario: Preparar liga el contenido al envío
- **WHEN** se prepara un envío con dos cajas y tres etiquetas sueltas del origen
- **THEN** ese contenido queda comprometido en el envío y no puede entrar a otro

#### Scenario: Contenido ajeno o comprometido se rechaza
- **WHEN** se intenta preparar un envío con una caja de otra sucursal o con una etiqueta ya comprometida en otro envío
- **THEN** el sistema rechaza la operación

#### Scenario: Cancelar libera el contenido
- **WHEN** se cancela un envío en estado preparado
- **THEN** sus cajas y etiquetas quedan libres para otro envío y el documento queda cancelado con su folio

#### Scenario: Un envío ya enviado no se cancela
- **WHEN** se intenta cancelar un envío en estado enviado o recibido
- **THEN** el sistema rechaza la operación

### Requirement: Ciclo de estados del envío
El envío SHALL transitar `preparado → enviado → recibido`, con marca de tiempo y actor auditados en cada transición. Marcar como enviado SHALL exigir `enviar` + alcance sobre el origen; recibir SHALL exigir el permiso `recibir` + alcance sobre el **destino**. Ninguna transición fuera de ese orden SHALL permitirse (no se recibe lo no enviado; no se re-recibe).

#### Scenario: Ciclo completo válido
- **WHEN** un envío preparado se marca enviado y después el destino lo recibe
- **THEN** cada transición queda registrada con su momento y su actor

#### Scenario: Recibir un envío no enviado se rechaza
- **WHEN** se intenta recibir un envío que sigue en estado preparado
- **THEN** el sistema rechaza la operación

#### Scenario: Recibir dos veces se rechaza
- **WHEN** se intenta recibir un envío ya recibido
- **THEN** el sistema rechaza la operación

#### Scenario: Recibir sin permiso o fuera del destino se rechaza
- **WHEN** un usuario sin `recibir` (o cuyo alcance no cubre el destino) intenta recibir un envío enviado
- **THEN** el sistema rechaza la operación

### Requirement: Efectos de inventario asimétricos del envío
La matriz SHALL NO postear movimientos de inventario (no lleva stock propio). En el **surtido** (matriz→expendio), marcar enviado SHALL NO tocar stock, y recibir SHALL postear en el expendio **un movimiento `recepcion` por producto** con lo presente (etiquetas en gramos, cajas de pieza en unidades), citando el folio del envío, actualizando el saldo en la misma operación atómica. En la **devolución** (expendio→matriz), marcar enviado SHALL postear en el expendio **un movimiento `envio` por producto** (salida) sujeto a la no-negatividad de la existencia, y recibir en matriz SHALL NO postear movimientos.

#### Scenario: El stock del expendio nace al recibir el surtido
- **WHEN** un expendio recibe un envío con 3 etiquetas (2 500 g) de un producto y una caja de 24 piezas de otro
- **THEN** su existencia queda en 2 500 g y 24 unidades respectivamente, con un movimiento `recepcion` por producto citando el folio

#### Scenario: La matriz nunca postea movimientos
- **WHEN** se completa un surtido y una devolución que tocan a la matriz
- **THEN** la matriz no registra ningún movimiento de inventario ni existencia

#### Scenario: La devolución descuenta al expendio al enviar
- **WHEN** un expendio con 24 unidades marca enviada una devolución con una caja de 24
- **THEN** su existencia baja a 0 mediante un movimiento `envio` en ese momento

#### Scenario: No se devuelve más de lo que hay
- **WHEN** marcar enviada una devolución dejaría la existencia del expendio por debajo de cero
- **THEN** el sistema rechaza la operación y nada cambia

### Requirement: Las etiquetas viajan con el envío conservando identidad
Al **recibir**, las cajas y etiquetas **presentes** SHALL cambiar a la sucursal **destino** conservando su identidad (código, discriminador) y su **caducidad** original. Con ello, el stock recibido en un expendio SHALL quedar sujeto a la alerta "por vencer" de la capacidad `labeling`.

#### Scenario: La etiqueta recibida está en el destino con su caducidad
- **WHEN** un expendio recibe una etiqueta emitida en matriz
- **THEN** el lookup local la resuelve en el expendio con el mismo código y la misma caducidad

#### Scenario: Recibir enciende la alerta de caducidad
- **WHEN** un expendio recibe etiquetas cuya caducidad entra en la ventana de 5 días
- **THEN** el barrido "por vencer" las detecta y notifica (antes de recibirse, en matriz, no alertaban)

### Requirement: Recepción de lo real con discrepancia registrada y notificada
El receptor SHALL confirmar **lo que llegó**: la lista de cajas (enteras) y etiquetas sueltas presentes. Lo declarado y no presente SHALL quedar en estado **extraviado**: fuera del origen y del destino, sin contar en ningún stock y sin generar alertas de caducidad. La discrepancia SHALL quedar **anotada en el documento** y SHALL disparar una notificación de tipo discrepancia a la bandeja de la **matriz** (administración). Una recepción completa SHALL NO generar notificación.

#### Scenario: Recibir con faltante registra y notifica
- **WHEN** el destino confirma 38 de las 40 etiquetas sueltas enviadas
- **THEN** las 38 quedan en el destino, las 2 quedan extraviadas y anotadas en el documento, y la administración recibe una notificación de discrepancia

#### Scenario: Lo extraviado no cuenta ni alerta
- **WHEN** una etiqueta queda extraviada en una recepción
- **THEN** no aparece en la existencia de ninguna sucursal ni en el barrido "por vencer"

#### Scenario: Recepción completa sin ruido
- **WHEN** el destino confirma exactamente todo el contenido enviado
- **THEN** el envío queda recibido sin notificación de discrepancia
