# cash-management Specification

## Purpose

Define la gestión de efectivo de ToyPOS: registro de **gastos** (todos sujetos a aprobación) y el **corte de caja** por sesión. Cada sucursal maneja una caja (un cajero = una caja) que se **abre** declarando fondo y se **cierra** capturando el efectivo contado, con **cierre a ciegas** (el cajero no ve esperado ni diferencia). El sistema **muestra** la diferencia **sin sancionar**: solo los gastos autorizados restan del esperado, y los no autorizados aparecen como faltante que revisa el administrador. Un corte cerrado es **inmutable**; los gastos resueltos tras el cierre se **concilian** ligándolos a su sesión sin reescribir cifras históricas. Los importes se guardan **exactos en centavos** (sin redondeo) y cada documento recibe un **folio** legible, único y secuencial por sucursal y tipo.

## Requirements

### Requirement: Registro de gastos sujeto a aprobación
El sistema SHALL permitir registrar gastos de una sucursal con al menos: monto, concepto y quién lo registra. Todo gasto SHALL requerir aprobación (ver capacidad `authorization-workflow`) y SHALL nacer en estado no autorizado.

#### Scenario: Gasto nace pendiente de aprobación
- **WHEN** un cajero registra un gasto de 2000.00 por "publicidad"
- **THEN** el gasto queda no autorizado y se crea una aprobación pendiente dirigida a quien pueda autorizarla

### Requirement: Apertura de la sesión de caja
El sistema SHALL abrir la sesión de caja de una **sucursal** (una caja por sucursal: un cajero = una caja) cuando el cajero de turno declara su **fondo de apertura** (el efectivo de cambio con el que inicia); la sesión SHALL registrar al cajero que la abre. El fondo SHALL declararse en cada apertura, SHALL variar libremente entre días y SHALL NO arrastrarse de una sesión a la siguiente. La sesión abierta define el período operativo ("el día") contra el que se registran ventas y gastos, hasta su cierre. El sistema SHALL permitir **a lo sumo una sesión abierta a la vez por sucursal**: SHALL NO abrir una nueva sesión mientras exista una **sin cerrar** en esa sucursal; el cajero SHALL cerrar la sesión pendiente antes de abrir la siguiente (no se abre el corte del martes sin cerrar el del lunes). Cada sucursal SHALL gestionar sus cortes de forma **independiente** de las demás. Los días **sin labor** simplemente no tienen sesión, y esa ausencia SHALL NO constituir una sesión pendiente.

#### Scenario: Apertura con fondo declarado
- **WHEN** el cajero abre su sesión declarando 200.00 de fondo de cambio
- **THEN** la sesión queda abierta con fondo 200.00 y las ventas y gastos posteriores se registran contra ella

#### Scenario: El fondo no se arrastra al día siguiente
- **WHEN** el cajero abre una nueva sesión al día siguiente y declara 500.00 de fondo
- **THEN** la nueva sesión inicia con fondo 500.00, sin relación con el fondo anterior

#### Scenario: No se abre una nueva sesión con otra sin cerrar
- **WHEN** el cajero intenta abrir su sesión y aún tiene una sesión anterior sin cerrar en esa sucursal
- **THEN** el sistema rechaza la apertura hasta que cierre la sesión pendiente

#### Scenario: Un día sin labor no deja sesión pendiente
- **WHEN** un día no se labora (no se abre sesión) y en el siguiente día laboral el cajero abre su sesión
- **THEN** el sistema permite la apertura, pues no existe una sesión abierta previa que cerrar

### Requirement: Cierre de la sesión de caja (corte)
El sistema SHALL cerrar la sesión cuando el cajero captura su **efectivo contado**. El efectivo esperado SHALL calcularse como: fondo de apertura + ventas en efectivo − gastos autorizados de la sesión. El **fondo NO cuenta como venta**: la venta neta en efectivo son las ventas cobradas en efectivo (sin el fondo). Cada venta lleva un método de pago (`efectivo | transferencia | deposito`, extensible); el corte SHALL mostrar el total de ventas de todos los métodos, pero solo las ventas en efectivo alimentan el arqueo físico, y las de transferencia o depósito SHALL mostrarse como informativas. El sistema SHALL **mostrar** la diferencia entre esperado y contado **sin aplicar sanción automática** alguna; la revisión y la decisión corresponden al administrador. Durante el cierre el cajero SHALL capturar únicamente su efectivo contado y SHALL NO ver el esperado ni la diferencia (**cierre a ciegas**): ver la conciliación requiere un permiso que el cajero no tiene.

#### Scenario: Cálculo del efectivo esperado y la venta neta
- **WHEN** se cierra con fondo 500, ventas en efectivo 8000 y gastos autorizados 1200
- **THEN** el efectivo esperado es 7300 y la venta neta en efectivo es 8000 (el fondo de 500 no es venta)

#### Scenario: Tarjeta y depósito se muestran pero no cuentan como efectivo
- **WHEN** la sesión tiene 8000 en ventas en efectivo y 3000 en ventas con transferencia o depósito
- **THEN** el corte muestra ventas por 11000 y el efectivo esperado solo considera los 8000 en efectivo

#### Scenario: El corte muestra la diferencia; el admin decide
- **WHEN** el efectivo esperado es 7300 y el contado es 7250
- **THEN** el corte muestra un faltante de 50 para revisión del administrador, sin cobrar nada al cajero

#### Scenario: Cierre a ciegas
- **WHEN** el cajero cierra su sesión
- **THEN** captura su efectivo contado sin que el sistema le muestre el esperado ni la diferencia

### Requirement: Solo los gastos autorizados restan del efectivo esperado
El sistema SHALL restar del efectivo esperado únicamente los gastos en estado autorizado a la fecha del corte; los gastos no autorizados SHALL NO reducir el efectivo esperado.

#### Scenario: Solo cuentan los autorizados
- **WHEN** en el día hay un gasto autorizado de 1200 y otro no autorizado de 300
- **THEN** el corte resta 1200 y no resta el de 300 del efectivo esperado

### Requirement: El sistema no sanciona; muestra la diferencia y el admin decide
El sistema SHALL mostrar la diferencia del corte **sin aplicar ninguna sanción o cargo automático** al cajero; la revisión y la decisión corresponden al administrador. Como los gastos no autorizados no reducen el efectivo esperado, su monto **aparece como faltante** en el corte. El sistema SHALL NO mantener ventana de gracia: un gasto pendiente permanece pendiente hasta que el administrador lo apruebe o lo rechace.

#### Scenario: Gasto no autorizado aparece como faltante, sin cobro automático
- **WHEN** el cajero gastó 300.00 en un gasto pendiente o rechazado y cierra su sesión
- **THEN** el corte muestra un faltante de 300.00 para revisión del administrador, sin cobrar nada automáticamente

#### Scenario: Un gasto pendiente se resuelve sin reloj
- **WHEN** un gasto sigue pendiente al cierre de la sesión
- **THEN** permanece pendiente hasta que el administrador lo apruebe o rechace, sin ventana de gracia ni cobro automático

### Requirement: El corte cerrado es inmutable; conciliación posterior del gasto
Un corte, una vez cerrado, SHALL ser **inmutable**: su efectivo esperado, contado y diferencia SHALL NO modificarse por eventos posteriores. Un gasto SHALL pertenecer siempre a la **sesión en que se registró**. Si un gasto de una sesión ya cerrada se resuelve (autoriza, rechaza o cancela) **después** del cierre, el sistema SHALL NO alterar ese corte: la resolución SHALL sellarse con su propia fecha y SHALL quedar **ligada** al gasto y a su sesión de origen, de modo que el faltante histórico quede **explicado** sin reescribirse. El valor de los gastos para fines contables SHALL derivarse de los **registros de gasto** (monto, fecha, estado), no de recalcular cortes cerrados.

#### Scenario: Autorización tardía no altera el corte cerrado
- **WHEN** un gasto registrado el lunes quedó como faltante en el corte del lunes ya cerrado y se autoriza el miércoles
- **THEN** el corte del lunes conserva su esperado, contado y faltante originales, y la autorización queda ligada al gasto con fecha del miércoles

#### Scenario: El faltante histórico queda explicado, no borrado
- **WHEN** el administrador revisa el corte del lunes después de autorizar el gasto tardío `SNJG204`
- **THEN** ve el faltante original de 300 justificado por el gasto autorizado, sin que las cifras del corte hayan cambiado

#### Scenario: El gasto no salta al corte del día de su resolución
- **WHEN** el gasto del lunes se autoriza el miércoles
- **THEN** no se resta del corte del miércoles (pertenece a la sesión del lunes) y ese corte no se ve afectado

### Requirement: El sistema no redondea; el redondeo del cobro es físico
El sistema SHALL guardar y mostrar los importes **exactos en centavos**, sin redondear al peso. Como no circulan monedas de centavos, el redondeo del cobro es una acción **manual del cajero**; cualquier diferencia que produzca entra al corte como parte de la diferencia contra el conteo físico, a responsabilidad del cajero, **sin tolerancia especial**.

#### Scenario: El ticket muestra el importe exacto
- **WHEN** el total de una venta es 12.40
- **THEN** el sistema registra y muestra 12.40, y el cajero cobra en físico el peso que corresponda

### Requirement: Folio de los documentos de caja
Cada documento generado en caja (corte de una sesión, gasto) SHALL recibir un **folio** legible con formato `{código_sucursal}{tipo}{consecutivo}` todo junto, sin separadores (p. ej. `SNJC56` para el corte, `SNJG204` para el gasto), distinto del identificador técnico interno. El folio SHALL ser **único** y **secuencial por sucursal y por tipo** (los cortes de una sucursal se numeran aparte de sus gastos y aparte de los de otra sucursal), SHALL asignarse **al confirmarse** el documento y SHALL NO reutilizarse: un documento cancelado o revertido conserva su folio y su número no se recicla (ver D18).

#### Scenario: El corte recibe folio secuencial por sucursal
- **WHEN** se cierra el corte número 56 en la sucursal San Juan (código `SNJ`)
- **THEN** el corte queda con folio `SNJC56`, independiente de la numeración de cortes de otras sucursales

#### Scenario: Numeración independiente entre sucursales
- **WHEN** San Juan va en su corte `SNJC56` y Toluca en su corte `TOLC19`
- **THEN** cada sucursal mantiene su propia secuencia, sin relación entre ambas

#### Scenario: Un gasto cancelado conserva su folio
- **WHEN** un gasto con folio `SNJG204` se cancela
- **THEN** conserva el folio `SNJG204` y ese número no se reutiliza para otro gasto
