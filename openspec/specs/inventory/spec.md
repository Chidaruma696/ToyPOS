# inventory Specification

## Purpose

Define el **inventario por sucursal** de ToyPOS: una existencia por `producto × sucursal` expresada en la unidad natural del producto (`pieza` en unidades enteras, `peso_variable` en gramos), alimentada por **movimientos tipados append-only** con estado `aplicado | revertido`, auditoría de actor y actualización atómica del saldo. Cubre el ajuste manual absoluto con motivo (permiso `ajustar_inventario`), la reversión limpia de movimientos, la regla de no-negatividad y la consulta de existencias (permiso `ver_inventario`), siempre bajo alcance por sucursal. La definición del producto vive en `product-catalog`; los tipos de movimiento `recepcion` y `venta` quedan reservados para capacidades futuras.

## Requirements

### Requirement: Existencia por producto y sucursal
El sistema SHALL mantener una **existencia** por cada combinación de `producto × sucursal`, expresada en la **unidad natural del producto**: `pieza` en unidades enteras y `peso_variable` en gramos. La existencia de un producto que nunca ha tenido movimiento en una sucursal SHALL ser **cero** (no se materializan filas vacías). La existencia SHALL estar aislada por alcance: pertenece a su sucursal.

#### Scenario: Producto sin movimientos tiene existencia cero
- **WHEN** se consulta la existencia de un producto que nunca se movió en una sucursal
- **THEN** el sistema reporta existencia cero para esa sucursal

#### Scenario: La unidad de la existencia deriva del tipo de producto
- **WHEN** se consulta la existencia de un producto `peso_variable` y la de uno `pieza`
- **THEN** la del `peso_variable` se expresa en gramos y la del `pieza` en unidades enteras

#### Scenario: La existencia es por sucursal
- **WHEN** un producto tiene 10 unidades en la Sucursal A y 3 en la Sucursal B
- **THEN** cada sucursal reporta su propia existencia sin afectar a la otra

### Requirement: Movimientos de inventario tipados, append-only y auditados
Toda variación de existencia SHALL registrarse como un **movimiento** con: tipo (extensible; hoy `ajuste`, y `recepcion` y `venta` reservados para capacidades futuras), cantidad con signo en la unidad del producto, motivo cuando aplique, un **estado** `aplicado | revertido`, actor y marca de tiempo. El registro de movimientos SHALL ser **append-only**: un movimiento no se edita salvo el toggle de su estado al revertir (ver reversión), y nunca se borra. El **saldo** de la existencia SHALL actualizarse en la **misma operación atómica** que registra o togglea un movimiento, de modo que el saldo siempre iguale la suma de los deltas de los movimientos **aplicados** (vigentes). Cada movimiento SHALL quedar atribuido a su actor en la bitácora de auditoría (usuario o `sistema`).

#### Scenario: Un movimiento actualiza el saldo atómicamente
- **WHEN** se aplica un movimiento de +5 unidades sobre una existencia de 10
- **THEN** el movimiento queda registrado y la existencia pasa a 15 en la misma operación

#### Scenario: El saldo iguala la suma de los movimientos
- **WHEN** se aplican varios movimientos sucesivos sobre una existencia
- **THEN** el saldo reportado es exactamente la suma de los movimientos vigentes

#### Scenario: Un movimiento queda atribuido
- **WHEN** un usuario aplica un movimiento de inventario
- **THEN** la bitácora registra quién lo hizo, sobre qué producto y sucursal, y el cambio de saldo

### Requirement: Ajuste manual absoluto de existencias con motivo
El sistema SHALL permitir **ajustar** la existencia de un producto en una sucursal fijándola a una **cantidad contada** (valor absoluto) —para el alta inicial de stock y para correcciones tras un conteo físico— exigiendo el permiso `ajustar_inventario` y un alcance que cubra la sucursal. El ajuste SHALL requerir un **motivo** no vacío y SHALL producir un movimiento de tipo `ajuste`, cuyo delta es la diferencia contra el saldo previo, atribuido a su actor. No existe un ajuste por delta: una baja (merma o devolución de mercancía no vendida) se expresa recontando al nuevo valor.

#### Scenario: Ajuste con permiso y alcance
- **WHEN** un usuario con `ajustar_inventario` y alcance sobre la sucursal fija la existencia a la cantidad contada, con un motivo
- **THEN** el sistema registra el movimiento `ajuste` con el delta resultante y el motivo, y deja la existencia en la cantidad contada

#### Scenario: Ajuste sin permiso
- **WHEN** un usuario sin `ajustar_inventario` intenta ajustar una existencia
- **THEN** el sistema rechaza la operación

#### Scenario: Ajuste fuera del alcance
- **WHEN** un usuario con `ajustar_inventario` pero cuyo alcance no cubre la sucursal intenta ajustar su existencia
- **THEN** el sistema rechaza la operación

#### Scenario: Ajuste sin motivo
- **WHEN** se intenta ajustar una existencia sin indicar un motivo
- **THEN** el sistema rechaza la operación

### Requirement: La existencia no puede ser negativa
La existencia SHALL ser siempre mayor o igual a cero. Un movimiento de salida o un ajuste que dejaría la existencia por debajo de cero SHALL rechazarse, sin alterar el saldo ni registrar el movimiento.

#### Scenario: Salida que excede lo disponible se rechaza
- **WHEN** se intenta descontar 12 unidades de una existencia de 10
- **THEN** el sistema rechaza la operación y la existencia permanece en 10

#### Scenario: Ajuste a un valor negativo se rechaza
- **WHEN** se intenta ajustar una existencia a un valor menor que cero
- **THEN** el sistema rechaza la operación

### Requirement: Reversión de un movimiento de inventario
El sistema SHALL permitir **revertir** un movimiento de ajuste: la reversión SHALL registrar un movimiento compensatorio que deshace exactamente el efecto sobre el saldo, SHALL marcar el movimiento original como `revertido` sin borrarlo, y SHALL ser reversible a su vez (`aplicado ⇄ revertido`) alternando limpiamente sin duplicar la entidad. Cada alternancia SHALL quedar auditada. La existencia resultante SHALL reflejar el estado vigente.

#### Scenario: Revertir restaura el saldo y conserva la historia
- **WHEN** se revierte un ajuste de +5 que había llevado la existencia de 10 a 15
- **THEN** la existencia vuelve a 10, el movimiento original queda `revertido` y tanto él como su compensación permanecen en el registro

#### Scenario: Rehacer vuelve a aplicar sin duplicar
- **WHEN** se revierte y luego se vuelve a aplicar el mismo movimiento
- **THEN** la existencia refleja el estado vigente y no se apilan copias del movimiento

#### Scenario: Una reversión no puede dejar la existencia negativa
- **WHEN** revertir un movimiento de entrada dejaría la existencia por debajo de cero
- **THEN** el sistema rechaza la reversión y el saldo no cambia

### Requirement: Consulta de existencia dentro del alcance
El sistema SHALL exponer la consulta de la existencia actual de un producto en una sucursal, para los consumidores (p. ej. el punto de venta futuro), exigiendo el permiso `ver_inventario` y un alcance que cubra la sucursal. Un usuario SHALL NO poder consultar la existencia de una sucursal fuera de su alcance.

#### Scenario: Consulta con permiso y alcance
- **WHEN** un usuario con `ver_inventario` y alcance sobre la sucursal consulta la existencia de un producto
- **THEN** el sistema devuelve la cantidad actual en la unidad del producto

#### Scenario: Consulta fuera del alcance
- **WHEN** un usuario consulta la existencia de una sucursal que su alcance no cubre
- **THEN** el sistema rechaza la operación
