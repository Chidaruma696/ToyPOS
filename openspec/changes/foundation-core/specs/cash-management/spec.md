## ADDED Requirements

### Requirement: Registro de gastos sujeto a autorización
El sistema SHALL permitir registrar gastos de una sucursal con al menos: monto, concepto y quién lo registra. Todo gasto SHALL requerir autorización mediante una solicitud de tipo `gasto` del subsistema de autorizaciones, y SHALL nacer en estado no autorizado.

#### Scenario: Gasto crea solicitud pendiente
- **WHEN** un cajero registra un gasto de 2000.00 por "publicidad"
- **THEN** el gasto queda no autorizado y se crea una solicitud `gasto` pendiente dirigida a quien pueda autorizarla

### Requirement: Corte de caja diario
El sistema SHALL producir un corte de caja diario por cajero y sucursal. El efectivo esperado SHALL calcularse como: fondo inicial + entradas de efectivo del día − gastos autorizados del día. (Las entradas de efectivo provienen de las ventas, una capacidad futura; el corte se define sobre esa entrada.)

#### Scenario: Cálculo del efectivo esperado
- **WHEN** se realiza el corte con fondo 500, entradas de efectivo 8000 y gastos autorizados 1200
- **THEN** el efectivo esperado es 7300

#### Scenario: Diferencia contra el conteo físico
- **WHEN** el efectivo esperado es 7300 y el efectivo contado es 7250
- **THEN** el corte reporta una diferencia (faltante) de 50 a cargo del cajero

### Requirement: Solo los gastos autorizados restan del efectivo esperado
El sistema SHALL restar del efectivo esperado únicamente los gastos en estado autorizado a la fecha del corte; los gastos no autorizados SHALL NO reducir el efectivo esperado.

#### Scenario: Solo cuentan los autorizados
- **WHEN** en el día hay un gasto autorizado de 1200 y otro no autorizado de 300
- **THEN** el corte resta 1200 y no resta el de 300 del efectivo esperado

### Requirement: Gasto no autorizado se cobra al cajero
El sistema SHALL imputar como cargo al cajero (faltante a su cuenta) todo gasto que no quede autorizado. Un gasto pendiente al momento del corte SHALL disponer de una ventana de gracia configurable, cuyo valor por defecto SHALL mantener el gasto en suspenso hasta el corte del día siguiente; si al corte siguiente sigue sin autorizarse, SHALL cobrarse al cajero.

#### Scenario: Gasto rechazado se cobra
- **WHEN** un gasto es rechazado por el aprobador
- **THEN** su monto se imputa como cargo al cajero que lo registró

#### Scenario: Gasto pendiente en ventana de gracia
- **WHEN** un gasto sigue pendiente al corte del día en que se registró
- **THEN** no se cobra aún al cajero y permanece pendiente hasta el corte siguiente

#### Scenario: Pendiente vencido se cobra
- **WHEN** un gasto sigue pendiente al corte del día siguiente
- **THEN** su monto se imputa como cargo al cajero
