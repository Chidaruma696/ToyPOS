## ADDED Requirements

### Requirement: Un gasto requiere aprobación
El sistema SHALL exigir que todo gasto sea aprobado antes de contar como autorizado. Al registrarse un gasto, el sistema SHALL crear una **aprobación** en estado `pendiente`, asociada al gasto y al alcance de su sucursal.

#### Scenario: El gasto nace pendiente de aprobación
- **WHEN** un cajero registra un gasto en su sucursal
- **THEN** el sistema crea una aprobación `pendiente` para ese gasto, con el alcance de la sucursal

### Requirement: Ciclo de la aprobación
Una aprobación SHALL iniciar en `pendiente` y transicionar a `aprobada`, `rechazada` o `cancelada`. Los tres estados son terminales e inmutables. Cada resolución SHALL registrar quién resolvió y cuándo; **el rechazo y la cancelación SHALL registrar además un motivo**. (La máquina de estados se mantiene como pieza reutilizable; se generalizará solo si aparece un segundo flujo **real de aprobación** —hoy no hay ninguno: el traspaso es un *envío* y el pedido es un *surtido*, no aprobaciones.)

#### Scenario: Aprobación registra autoría
- **WHEN** un aprobador aprueba un gasto pendiente
- **THEN** la aprobación pasa a `aprobada` y se registran el aprobador y la marca de tiempo

#### Scenario: Rechazo registra motivo
- **WHEN** un aprobador rechaza un gasto pendiente
- **THEN** la aprobación pasa a `rechazada` y se registran el aprobador, la marca de tiempo y el motivo del rechazo

#### Scenario: No se re-resuelve una aprobación terminal
- **WHEN** se intenta resolver de nuevo un gasto ya `aprobado`, `rechazado` o `cancelado`
- **THEN** el sistema rechaza la operación

### Requirement: El aprobador se determina por permiso y alcance
El sistema SHALL exigir que quien resuelve la aprobación de un gasto tenga el permiso `autorizar_gasto` y un alcance que cubra la sucursal del gasto. El permiso `autorizar_gasto` es una **autoridad administrativa**: autorizar un gasto corresponde al **administrador, nunca al cajero**. El rol de cajero —que registra gastos y opera la caja— SHALL NO incluir `autorizar_gasto`; en consecuencia un cajero SHALL NO poder autorizar gasto alguno, ni propio ni de otro, con independencia de la segregación de deberes.

#### Scenario: Aprobador con permiso y alcance suficiente
- **WHEN** un usuario con `autorizar_gasto` y alcance que cubre la sucursal resuelve un gasto pendiente
- **THEN** el sistema le permite aprobar o rechazar

#### Scenario: Alcance insuficiente
- **WHEN** un usuario con `autorizar_gasto` pero alcance limitado a otra sucursal intenta resolver el gasto
- **THEN** el sistema rechaza la operación

#### Scenario: El cajero no autoriza gastos
- **WHEN** un cajero (sin `autorizar_gasto`) intenta autorizar un gasto, sea propio o de otro cajero
- **THEN** el sistema rechaza la operación: autorizar es autoridad administrativa

### Requirement: Efecto de la resolución sobre el gasto
El sistema SHALL marcar como `autorizado` el gasto cuya aprobación pase a `aprobada` (resta del efectivo esperado del corte). Un gasto `rechazado` o aún `pendiente` SHALL NO contar como autorizado: no reduce el esperado y, por tanto, aparece como faltante en el corte, sin cobro automático (lo revisa el administrador). El efecto SHALL aplicarse sobre la **sesión de origen** del gasto (aquella en que se registró). Si esa sesión **ya cerró** su corte al momento de la resolución, el corte cerrado SHALL NO alterarse: la resolución queda **ligada** al gasto con su propia fecha y explica el faltante histórico sin reescribirlo (conciliación posterior; ver capacidad `cash-management` y D19).

#### Scenario: Gasto aprobado cuenta en el corte
- **WHEN** la aprobación de un gasto pasa a `aprobada` mientras su sesión sigue abierta
- **THEN** el gasto queda autorizado y resta del efectivo esperado de ese corte

#### Scenario: Gasto rechazado no reduce el esperado
- **WHEN** la aprobación de un gasto pasa a `rechazada`
- **THEN** el gasto no cuenta como autorizado; su monto no se resta y aparece como faltante que el administrador revisa

#### Scenario: Resolución tardía no reescribe el corte cerrado
- **WHEN** un gasto registrado en una sesión ya cerrada se autoriza después del cierre de su corte
- **THEN** el corte cerrado conserva sus cifras y la autorización queda ligada al gasto con su fecha, sin modificar valores históricos

### Requirement: Segregación de deberes en la aprobación
El sistema SHALL impedir que el solicitante de una aprobación la resuelva él mismo: no puede aprobar ni rechazar su propia solicitud. La única excepción SHALL ser un administrador que tenga un permiso de auto-aprobación (de-sistema, p. ej. `autoaprobar_gasto`); en cualquier otra circunstancia la auto-aprobación SHALL rechazarse.

#### Scenario: Un usuario no aprueba su propio gasto
- **WHEN** el cajero que registró un gasto intenta aprobarlo él mismo
- **THEN** el sistema rechaza la operación por segregación de deberes

#### Scenario: El administrador habilitado sí puede auto-aprobar
- **WHEN** un administrador con `autoaprobar_gasto` resuelve un gasto que él mismo registró
- **THEN** el sistema lo permite

### Requirement: Cancelación de una solicitud pendiente
El sistema SHALL permitir **cancelar** una solicitud mientras esté `pendiente`, para anular un gasto registrado por error. La cancelación SHALL realizarla un aprobador (con `autorizar_gasto` + alcance que cubra la sucursal), **no el propio solicitante**, y SHALL registrar un motivo. Una solicitud `cancelada` SHALL NO surtir efecto: el gasto queda anulado y no cuenta en el corte. Una solicitud ya resuelta (`aprobada` o `rechazada`) SHALL NO poder cancelarse.

#### Scenario: El admin cancela, con motivo, un gasto registrado por error
- **WHEN** el administrador cancela con motivo un gasto pendiente que el cajero registró por accidente
- **THEN** la solicitud pasa a `cancelada`, el gasto queda anulado y no aparece en el corte

#### Scenario: El solicitante no puede cancelar su propia solicitud
- **WHEN** el cajero que registró un gasto intenta cancelarlo él mismo
- **THEN** el sistema rechaza la operación (la cancelación la resuelve un aprobador, no el solicitante)

#### Scenario: No se cancela una solicitud ya resuelta
- **WHEN** se intenta cancelar un gasto ya `aprobado` o `rechazado`
- **THEN** el sistema rechaza la operación
