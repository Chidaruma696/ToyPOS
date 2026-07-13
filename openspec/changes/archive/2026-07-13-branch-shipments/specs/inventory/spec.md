## MODIFIED Requirements

### Requirement: Movimientos de inventario tipados, append-only y auditados
Toda variación de existencia SHALL registrarse como un **movimiento** con: tipo (extensible; hoy `ajuste`, `envio` y `recepcion` — posteados por el ajuste manual y por la capacidad `shipments` — y `venta` reservado para el POS futuro), cantidad con signo en la unidad del producto, motivo cuando aplique, un **estado** `aplicado | revertido`, actor y marca de tiempo. El registro de movimientos SHALL ser **append-only**: un movimiento no se edita salvo el toggle de su estado al revertir (ver reversión), y nunca se borra. El **saldo** de la existencia SHALL actualizarse en la **misma operación atómica** que registra o togglea un movimiento, de modo que el saldo siempre iguale la suma de los deltas de los movimientos **aplicados** (vigentes). Cada movimiento SHALL quedar atribuido a su actor en la bitácora de auditoría (usuario o `sistema`).

#### Scenario: Un movimiento actualiza el saldo atómicamente
- **WHEN** se aplica un movimiento de +5 unidades sobre una existencia de 10
- **THEN** el movimiento queda registrado y la existencia pasa a 15 en la misma operación

#### Scenario: El saldo iguala la suma de los movimientos
- **WHEN** se aplican varios movimientos sucesivos sobre una existencia
- **THEN** el saldo reportado es exactamente la suma de los movimientos vigentes

#### Scenario: Un movimiento queda atribuido
- **WHEN** un usuario aplica un movimiento de inventario
- **THEN** la bitácora registra quién lo hizo, sobre qué producto y sucursal, y el cambio de saldo
