## Context

ToyPOS es un POS multi-sucursal con topología **matriz → expendios afiliados**: la matriz produce, empaqueta y gobierna; los expendios reciben y venden. El sistema tendrá tres caras (etiquetado/pesaje, punto de venta, panel de administración) sobre un dominio compartido. Este cambio no construye ninguna de las tres caras: asienta el **núcleo de dominio** del que todas dependen — identidad/acceso, catálogo de producto, precios, autorizaciones y efectivo.

Restricciones fijadas durante la exploración:
- Backend en **Rust + Tokio** (rápido, no bloqueante).
- Front en **Tauri + Vue** (ligero para PCs modestas, acceso nativo a hardware, cross-platform).
- **Windows-primario** por drivers de impresora; Linux soportado por el mismo binario Rust/Tauri.
- Conectividad **online con degradación offline**: la ruta caliente resuelve contra datos locales y sincroniza en segundo plano; vender no debe frenarse si cae la red.
- El repositorio está vacío: esta es la primera capa de código.

## Goals / Non-Goals

**Goals:**
- Un **modelo de dominio** correcto y tipado (usuario, sucursal, producto peso/pieza, precio, gasto, corte, solicitud de autorización).
- **RBAC modular**: permisos atómicos → roles a medida → alcance. Que el **alcance sea el aislamiento** entre sucursales, aplicado en la capa de datos.
- Un **subsistema de autorizaciones genérico** (solicitud → aprobada/rechazada) reutilizable, con gastos como primer consumidor.
- **Precios** por producto × sucursal × nivel, con copiar y estado congelado.
- **Corte de caja** diario con la regla de cargo al cajero por gasto no autorizado.
- Estructura de proyecto (workspace) que permita que las tres caras reutilicen el mismo core sin duplicar reglas.

**Non-Goals:**
- Etiquetado con báscula, formato EAN-13 y discriminador antiduplicado.
- Cajas, traspaso físico, recepción con validación de completitud.
- Checkout / punto de venta y su UI.
- Sincronización offline real (motor de sync, resolución de conflictos).
- Panel de estadísticas del admin.
- Integración de hardware (báscula, impresoras, escáner).
- Retiros/depósitos de efectivo a banco: movimiento físico que el sistema no puede verificar (no sabe cómo ni qué se depositó).

## Principios rectores

Todo el sistema —dominio, datos e interfaces— se construye **estrictamente** bajo:
- **KISS** — la solución más simple que funcione; si una pieza se puede quitar y sigue cumpliendo, se quita.
- **DRY** — cada regla vive una sola vez (para eso existe el crate `domain`).
- **YAGNI** — solo se construye lo que tiene un consumidor real hoy o planeado; nada "por si acaso". Se distingue *correctitud faltante* (se cierra) de *flexibilidad especulativa* (se difiere).
- **Kanso (簡素)** — estética zen para las tres caras (etiquetado, POS, admin): simplicidad por eliminación, sin desorden; cada elemento debe ganar su lugar.

Estos principios son el filtro de **toda** decisión y de todo cambio futuro, incluida la revisión de las propias propuestas (retractar la sobre-ingeniería).

## Decisions

### D1 — Workspace: un core de dominio, tres despliegues
Un **workspace de Cargo** con un crate `domain` (entidades + reglas puras, sin I/O) del que colgarán, en cambios futuros, los binarios de etiquetado, POS y admin. La lógica "precio × cantidad", permisos y estados de solicitud vive **una sola vez** en `domain`.
- *Alternativa descartada:* tres aplicaciones independientes → reimplementarían reglas y divergirían.

### D2 — Autorización basada en permisos, no en roles fijos
La autorización se evalúa contra **permisos atómicos**; los roles son solo bundles nombrados y editables. Esto satisface el requisito de "ponerle el permiso de precios al etiquetador de forma modular" sin tocar código.
- *Alternativa descartada:* enum de roles cerrado (`Admin | Cajero | Etiquetador`) → cada cambio organizativo exigiría recompilar.

### D3 — Dos ejes ortogonales: permiso (*qué*) y alcance (*dónde*); aislamiento en la capa de datos
El acceso se decide con **dos ejes independientes**:
- **Permisos** (*qué*): una **bolsa plana** por usuario (unión de sus roles), a nivel de capacidad. Cada permiso es **por-sucursal** o **de-sistema**.
- **Alcance** (*dónde*): un **único conjunto de sucursales por usuario** (una, varias, o todas = nivel matriz). No va por rol ni por permiso.

El chequeo es `tiene(permiso) ∧ alcance_cubre(sucursal)` para permisos por-sucursal, y solo `tiene(permiso)` para los de-sistema. Con un **solo alcance por usuario** no hay fuga (la fuga clásica surge al tener varios alcances y unirlos por separado — aquí no existe). Todo se aplica en la **capa de acceso a datos** vía un `ContextoAcceso { actor, permisos, alcance }` que cada repositorio exige; la UI (el *switch* de sucursal) es solo azúcar sobre esta reja.
- *Actor = usuario **o** sistema:* casi siempre el actor es el usuario de la sesión; para las escrituras **sin usuario** —la **bootstrap** que siembra matriz/admin (D3.10) y las **operaciones automáticas** futuras (p. ej. el auto-reabasto)— el actor es **`sistema`**, con alcance según la operación. Así ninguna escritura, humana o no, queda sin atribuir en la bitácora (D11), y el log distingue lo automático de lo humano.
- *"Matriz" como alcance = todas las sucursales, y nada más:* no concede permisos por sí mismo (un usuario de matriz puede no tener `autorizar_gasto`).
- *Alternativa descartada:* filtrar en la UI/handlers → una ruta olvidada = fuga entre sucursales.
- *Alternativa descartada:* acoplar permiso↔sucursal por asignación → complejidad para una granularidad (poderes distintos por sucursal para la misma persona) que el negocio no necesita (YAGNI).

### D4 — Aprobación de gastos concreta, no un subsistema genérico [KISS/YAGNI]
La aprobación es **concreta para gastos**: una `Aprobacion { gasto, alcance, estado, resolutor, resuelto_en, motivo }` con máquina de estados `pendiente → aprobada|rechazada|cancelada` (rechazo y cancelación con motivo; la cancelación —para gastos por error— la resuelve un aprobador, no el solicitante). **No** se construye la parametrización genérica por `tipo`/`payload`/efecto delegado: hoy hay un solo consumidor real (gastos). La **máquina de estados** queda como pieza reutilizable (DRY); se generaliza **solo si aparece un segundo flujo realmente de aprobación** — y hasta ahora ninguno lo es: el **traspaso** resultó ser *envío* (creada→enviada→recibida) y el **pedido** de reabasto es *surtido/backorder* (matriz no aprueba ni rechaza: surte total o parcialmente, y el faltante queda *pendiente de surtir* hasta completarse en las entregas que hagan falta — `solicitado → surtido_parcial* → surtido`). Ninguno es `pendiente→aprobada|rechazada`.
- *Alternativa descartada ahora:* subsistema genérico parametrizado por tipo con un único consumidor → flexibilidad especulativa (viola YAGNI); refactorizar a genérico cuando aparezca ese segundo consumidor de aprobación es barato si esto queda limpio.
- *Nota:* los **precios NO** pasan por aprobación (edición directa del admin).

### D5 — Precio = clave (producto, sucursal, nivel); "sin precio" es un estado derivado
El precio es una tabla con clave `(producto, sucursal, nivel)`. "Congelado" **no** es un flag persistido: es un estado **derivado** de la ausencia de precio de menudeo vigente en esa sucursal. Copiar precios es una operación masiva que crea/actualiza filas del destino sin tocar el origen.
- *Alternativa descartada:* un flag `congelado` persistido → se desincroniza del precio real.
- *Historial de precios:* la tabla **se sobrescribe** (no se versiona); el historial de cambios de la lista se reconstruye replayeando la **bitácora de auditoría** (D11), y lo realmente cobrado vive en las ventas — no se duplica (DRY/KISS).

### D6 — Unidades enteras extremo a extremo (nunca float)
- **Peso**: entero en **gramos** (= 3 decimales de kg, la precisión de la báscula **Torrey**). La misma unidad entera viaja sin conversión báscula → etiqueta → inventario → cuadre físico, de modo que el conteo y el sistema cuadran **sin desfase de gramos**. El peso SHALL NO redondearse nunca a 2 decimales (esa es la causa clásica del desfase).
- **Dinero**: entero en **centavos** (2 decimales). El importe de una venta por peso se calcula `precio_por_kg_centavos × gramos ÷ 1000` y se **redondea a centavos** (el efectivo siempre a 2 decimales, medio hacia arriba). El redondeo ocurre por línea de venta; el total es la suma de líneas ya redondeadas.
- *Alternativa descartada:* `f64` para dinero o peso → imprecisión acumulada inaceptable en cobros y cortes.

### D7 — Persistencia local embebida (SQLite), alineada con la ruta caliente local
La verdad operativa de cada nodo vive en **SQLite** embebido (vía `sqlx` o `rusqlite`), coherente con "la ruta caliente resuelve local". El motor de sincronización hacia la matriz es un **cambio futuro**; esta fundación deja el modelo de datos y los repositorios listos, pero no implementa sync.
- *Alternativa descartada ahora:* Postgres central como única verdad → rompe la degradación offline.
- *Objetivo de sync (previsto, no fijado):* **Supabase** (Postgres gestionado) como espejo central al que cada nodo sincroniza; el SQLite local sigue siendo la verdad de la ruta caliente. La elección concreta del motor de sync se decide en el cambio de sync.
- *Restricción del sync futuro:* corre en **segundo plano**, **nunca bloquea** la ruta caliente (vender/cobrar/cerrar corte) y debe ser **ligero** (poco CPU y ancho de banda); subir cambios es **oportunista**, no un requisito para operar.

### D8 — Precio desacoplado de la etiqueta
El código de barras de un producto de peso variable carga **solo el peso**; el precio/kg vive en la configuración de la sucursal y se resuelve en el POS. El precio SHALL NO imprimirse ni codificarse en la etiqueta.
- *Rationale:* el precio cambia con frecuencia; ponerlo en la etiqueta obligaría a reimprimir todo el stock etiquetado ante cada cambio.
- *Alternativa descartada:* barcode con precio embebido (price-embedded) → reimpresión en cada cambio de precio y etiqueta atada a una sola sucursal.

### D9 — El sistema guarda montos exactos; el redondeo del cobro es físico (sin tolerancia)
El sistema **nunca redondea al peso**: guarda y muestra el importe **exacto en centavos** (D6). El ticket sale con el monto exacto (p. ej. 12.40); como no circulan monedas de centavos, el **cajero cobra en físico** el peso entero que decida (12 o 13). Ese ajuste **no se modela**: es una acción manual, y cualquier diferencia que produzca entra al corte como parte de la **diferencia contra el conteo físico**, que el administrador revisa (sin sanción automática, D13).
- *Sin tolerancia (YAGNI):* no se construye una banda de tolerancia porque nadie la ha solicitado; si el negocio la pide, se agrega entonces.
- *Alternativa descartada:* que el sistema redondee al peso y lleve una línea "redondeo" en el corte → complejidad innecesaria (método de pago, doble ecuación del corte) para un ajuste que es manual.

### D10 — El corte separa las ventas por método de pago; solo el efectivo alimenta el arqueo
Cada entrada de venta del día lleva un **método de pago** (`efectivo | transferencia | deposito`, extensible). El corte **muestra el total de ventas del día** (todos los métodos) como información, pero el **efectivo esperado** solo suma las ventas en **efectivo**: transferencia y depósito no son dinero en el cajón y **no entran al arqueo físico**.
- *Rationale:* asumir que toda venta es efectivo inflaría el efectivo esperado y le cargaría al cajero un "faltante" por dinero que nunca tocó el cajón.
- *Fuera de alcance (futuro):* la conciliación propia de transferencia/depósito (contra terminal / estado de cuenta).
- *Alternativa descartada:* un corte que solo maneja efectivo y ni siquiera muestra las ventas con transferencia → el cajero/admin pierde visibilidad del total vendido.

### D11 — Bitácora de auditoría en el punto único de acceso a datos [aprovecha D3]
Toda escritura pasa por la capa de repositorio (D3); ahí mismo se registra una **bitácora append-only e inmutable** de cada operación **mutante** (crear/modificar/eliminar): actor, entidad, valores antes→después, alcance y marca de tiempo. Un solo mecanismo transversal (DRY) da rendición de cuentas a **todo** el sistema —incluidas capacidades futuras como inventario— sin logging por feature.
- *Motivación real:* en el sistema anterior un empleado desajustaba el inventario a propósito y no había forma de probar quién fue.
- *Actor humano o `sistema`:* el actor registrado es el usuario de la sesión o, en operaciones sin usuario (bootstrap ahora; automáticas futuras como el auto-reabasto), el actor **`sistema`** — un log más específico que separa lo que hizo una persona de lo que hizo el sistema, sin dejar ninguna escritura sin atribuir.
- *Alcance:* operaciones mutantes (C/U/D). Las lecturas no se auditan (volumen); se puede añadir auditoría de lecturas sensibles si se solicita.
- *No es sobre-ingeniería pese a ser amplio:* el costo es un único hook en el chokepoint que ya existe por D3.

### D12 — Esquema sync-ready y sin borrado físico; sync futuro a Supabase
Cada entidad nace con las convenciones que cualquier sync SQLite↔Supabase necesita y que **no se pueden reconstruir después**: **`uuid` como PK** (ya en 1.3), **`updated_at`**, y **sin hard delete** (borrado lógico / desactivación). Las tres pagan su lugar hoy —`updated_at` sirve para depurar, el borrado lógico protege la auditoría (D11)— y evitan una migración dolorosa al aterrizar el sync.
- *Previsto:* espejo central en **Supabase** (D7); la mecánica concreta (motor, resolución de conflictos, propagación de tombstones) se difiere al cambio de sync.
- *Difiere (YAGNI):* nodo de origen y demás metadatos que solo sirven al motor de sync.

### D13 — Ciclo de caja: sesión con apertura y cierre a ciegas; el sistema muestra, el humano decide
La unidad operativa es la **`SesiónDeCaja`** de la **sucursal** (**un cajero = una caja**, una caja por sucursal): **abre** cuando el cajero de turno declara su **fondo** (efectivo de cambio, autodeclarado, sin arrastre entre días) y **cierra** cuando captura su **efectivo contado**; la sesión **registra al cajero** que la opera (autoría). El corte de caja es el cierre de la sesión, y la **sesión —no el reloj— define "el día"** (#10): ventas y gastos cuentan contra la sesión abierta.
- **Corte diario y secuencial, por sucursal:** a lo sumo **una sesión abierta a la vez por sucursal** (su única caja). No se abre una nueva sesión si la anterior sigue abierta — hay que **cerrar antes de abrir**: no se puede abrir el corte del martes sin cerrar el del lunes. Los días **sin labor** simplemente no tienen sesión (no se abre corte); eso **no** rompe la secuencia — solo la rompe dejar una sesión sin cerrar. Esto obliga a que cada día laborado quede cuadrado antes de empezar el siguiente. **Cada sucursal gestiona sus cortes de forma independiente** (aislamiento por alcance, D3): lo que pase con la caja de un expendio no afecta ni bloquea a otro.
- **Cierre a ciegas:** el cajero captura solo lo contado y **no ve** el esperado ni la diferencia; ver la conciliación es un permiso del administrador — control antifraude que **reutiliza el modelo de permisos** (D3), sin mecanismo nuevo.
- **Sin sanción automática:** el sistema **calcula y muestra** la diferencia; el administrador revisa y decide. No hay "cargo al cajero" ni ledger de adeudos (YAGNI). Un gasto no autorizado ya aparece como faltante por la sola aritmética —**un único mecanismo** (DRY)—, no se programa ningún cobro.
- *Fondo ≠ venta:* el fondo se registra aparte y no cuenta como venta; la venta neta en efectivo son las ventas cobradas en efectivo (sin el fondo).
- *Sin ventana de gracia:* un gasto pendiente lo resuelve el admin al aprobar/rechazar, sin reloj.

### D14 — Multi-sesión concurrente, obligatoria y sin fallas
El sistema SHALL soportar **múltiples sesiones simultáneas** sin degradarse: varios usuarios operando a la vez en distintas sucursales, y varios roles a la vez en la misma (p. ej. el admin revisa mientras otro etiqueta). Cada sesión lleva su propio `ContextoAcceso` (D3) y, en caja, su propia `SesiónDeCaja` por cajero (D13), de modo que N sesiones coexisten sin interferirse. El backend async (Rust + Tokio) y el aislamiento transaccional del almacenamiento sostienen la concurrencia.
- *No existe "modo un-usuario":* la concurrencia es un requisito, no un extra.

### D15 — Correcciones por reversión reversible, nunca por borrado
Deshacer una operación (p. ej. una venta maliciosa) SHALL hacerse con una **reversión**: una operación compensatoria que anula los efectos (devuelve inventario, revierte el efectivo) dejando el **estado actual limpio**, pero conservando en el registro tanto la operación como cada reversión, atribuidas (quién, cuándo, por qué). Nunca se borra el dato. La reversión es **ella misma reversible**: la operación lleva un estado `aplicada ⇄ revertida` que alterna limpiamente; cada alternancia postea su movimiento compensatorio y se audita, pero **no duplica** la operación (se togglea una sola entidad, no se apilan copias). Así: deshago → limpio, rehago → limpio, sin duplicados, y el efecto siempre refleja el estado actual con historial completo.
- *Estado limpio, historia intacta:* el inventario/efectivo vuelven a lo correcto y la evidencia se conserva — indispensable para el empleado malicioso, y para el admin que revierte por accidente (común) y necesita rehacer sin duplicar.
- *Por qué no un stack/Ctrl-Z literal ni copias nuevas:* borrar rompe la auditoría (D11); apilar copias duplica en el sync (D12); un stack global es ambiguo con N sesiones (D14). Un toggle de estado + compensación es un append que sincroniza limpio y compone con la concurrencia.
- *UX:* se presenta como "Deshacer" / "Rehacer" con confirmación; por debajo es el toggle de estado.
- *Alcance:* las reversiones concretas (venta, inventario) son de sus capacidades futuras; aquí se fija solo el principio transversal.

### D16 — Autenticación local-first y sesión
La credencial es **uniforme**: usuario + contraseña, **hasheada** (argon2/bcrypt), nunca en texto plano. La validación es **local** (contra el SQLite del nodo) para funcionar sin red; el login arma el `ContextoAcceso` (permisos + alcance, D3). La sesión se cierra por completo tras cierto tiempo de **inactividad** — se vuelve con un **login completo**, sin PIN de desbloqueo (una sola forma de entrar, KISS).
- *Ventana de revocación offline:* se acepta; se reconcilia en el sync avisando al admin de la actividad posterior a la baja (D11/D12 dejan las migas; la lógica vive en la capa de sync futura).
- *Sucursal activa dentro del alcance:* alcance de una sola sucursal → activa sin selección; varias o todas (matriz) → se elige/filtra en la app (default matriz). La selección **nunca amplía** el alcance, solo filtra dentro de lo que D3 permite.
- *Alternativa descartada:* Supabase Auth como autoridad de login → es online/JWT, no sirve offline.
- *Alternativa descartada:* PIN + bloqueo de pantalla → dos caminos de entrada; con credencial uniforme y logout por inactividad basta (KISS).

### D17 — Rendimiento: la ruta caliente es imperceptible, incluso bajo carga extrema
El sistema SHALL sentirse **instantáneo** en la ruta caliente (escanear→precio, cobrar, registrar gasto, abrir/cerrar corte) y **no degradarse** bajo carga alta —muchas sesiones y operaciones simultáneas—. No es un extra: un POS lento cuesta ventas y paciencia del cajero. La velocidad se gana **por diseño**, no optimizando tarde, y se apoya en decisiones ya tomadas:
- **Local-first (D7):** la ruta caliente resuelve **en proceso** contra SQLite embebido, sin red; la latencia base es de disco local, no de un servidor. El trabajo no crítico (sync, reportes, reabasto automático) corre **fuera** de la ruta caliente y **nunca la bloquea**.
- **Async sin bloqueo (D14):** Rust + Tokio y el aislamiento transaccional sostienen N sesiones concurrentes sin interferencia ni degradación apreciable.
- **Aritmética entera pura (D6):** dinero y peso se calculan con enteros, sin punto flotante.
- **El hook de auditoría es barato (D11):** la bitácora es un *append* en la misma transacción, sin cómputo pesado — "auditar todo" **no** puede convertirse en el cuello de botella de la escritura.
- **Acceso a datos sin N+1 ni full-scans:** los repositorios se apoyan en índices sobre las claves calientes (GTIN, clave de precio `(producto, sucursal, nivel)`, alcance, `updated_at`); nada de recorrer tablas completas en la ruta caliente.
- *SLO concretos diferidos al POS:* los números duros (p. ej. escaneo→precio < X ms; cierre de corte < Y ms bajo Z sesiones) se fijan y se **prueban con carga** cuando exista el hot path real. La fundación fija la **arquitectura** que los hace alcanzables, no los umbrales.
- *Tensión reconocida (D11 ↔ velocidad):* "auditar toda escritura" y "escritura instantánea" solo conviven si el append de bitácora es trivial; por eso es un hook barato en el chokepoint que ya existe (D3), no un subsistema aparte.

### D18 — Folio de documentos: identificador humano, secuencial por sucursal y tipo [aparte del uuid]
Cada **documento generado** (nota de venta, corte de caja, gasto; a futuro envío, pedido, devolución) lleva —además de su `uuid` técnico (D12)— un **folio** legible: `{código_sucursal}{tipo}{consecutivo}` **todo junto, sin separadores**, p. ej. `SNJC56` (corte 56 de San Juan), `SNJN13280` (nota), `SNJG204` (gasto). El folio es lo que la gente **ve, imprime y cita**; el `uuid` es solo para la máquina y el sync. Solo los *documentos/transacciones* llevan folio — el catálogo (producto, rol, usuario) no.
- **Código de sucursal:** atributo **explícito, único e inmutable** que el admin asigna al crear la sucursal (2–4 caracteres alfanuméricos, p. ej. `SNJ`, `SNQ`, `TOL`). **No se deriva del nombre** —así San Juan y San Joaquín no colisionan— y al ser inmutable, renombrar la sucursal no altera folios históricos. El sistema rechaza códigos duplicados.
- **Consecutivo por (sucursal, tipo):** cada tipo de documento lleva su **propia secuencia en cada sucursal**; por eso San Juan va en el corte #56 y Toluca en el #19 sin relación. Se genera **local** en el nodo (una caja por sucursal = un solo escritor, D13) → sin coordinación central, offline-safe, y globalmente único porque el código de sucursal no colisiona.
- **Marcador de tipo extensible:** una letra por tipo (`C` corte, `N` nota, `G` gasto…); los tipos futuros suman su letra sin tocar los existentes. Como el tipo es letra y el consecutivo son dígitos, el folio pegado sigue siendo **inequívoco** al leerlo, teclearlo o buscarlo; el personal se capacita para leerlo de corrido.
- **Sin reutilización:** un documento cancelado o revertido (D15) **conserva** su folio; nunca se rellenan huecos ni se recicla un número (integridad de auditoría, D11). El folio se asigna **al confirmarse** el documento.
- *Alternativa descartada:* prefijo derivado del nombre → colisiona (San Juan/San Joaquín) y se rompe al renombrar.
- *Alternativa descartada:* un consecutivo único global → exige coordinación en línea, rompe offline (D7) y no da el "#56 por sucursal" que el negocio quiere.

### D19 — El corte cerrado es inmutable; el gasto resuelto tarde se concilia, no reescribe
Un corte, una vez **cerrado**, es una **foto histórica** de ese arqueo y **no cambia**. Un gasto nace en su sesión y **siempre pertenece a ella**: si se resuelve (aprueba/rechaza/cancela) **antes** del cierre, surte efecto en ese corte con normalidad. Si se resuelve **después** de que el corte cerró, el corte **conserva** su esperado/contado/diferencia; la resolución se sella con **su propia fecha** y queda **ligada** al gasto y a su sesión de origen, **explicando** el faltante sin reescribirlo — p. ej. *"faltante $300 justificado después por `SNJG204`, autorizado el miércoles"*.
- *Por qué inmutable:* un corte que cambia según cuándo se mira rompe la confianza y la auditoría (D11). El efectivo salió del cajón el día de la sesión, no el día de la autorización; su corte debe reflejar lo que se supo ese día.
- *La verdad contable del gasto vive en el gasto:* monto, fecha, estado y resolución están en el registro del gasto (con su folio, D18). El corte responde "¿cuadró el cajón ese día?"; el ledger de gastos responde "¿qué se gastó y si se autorizó?". Dos preguntas, dos registros — no se corrompe uno para responder el otro.
- *El cajero se exonera por conciliación, no por reescritura:* al ligarse la resolución, el faltante histórico queda justificado; el admin decide, sin sanción automática (D13).
- *Compone con:* la regla secuencial (D13, el corte cerró para poder abrir el siguiente) y la reversión (D15, se apilan eventos que explican, no se borra ni reescribe).
- *Alternativa descartada:* un modal que permita "restar al corte cerrado" → unos cortes se reescriben y otros no (inconsistente) y altera valores históricos; el mismo corte daría dos números según cuándo se mire.
- *Presentación (futuro):* la vista de conciliación que muestra "faltante justificado después" es de una cara futura; la fundación fija el **dato** (gasto ligado a su sesión + fecha de resolución) y la **regla** (no reescribir el corte cerrado).

### D20 — Operable al 100% por teclado, sin mouse [velocidad + resiliencia; realizado en las caras futuras]
Toda función SHALL ser alcanzable y operable **enteramente con teclado** —navegar, capturar, confirmar diálogos, resolver aprobaciones, abrir/cerrar corte—; **ninguna acción SHALL requerir mouse**. El mouse es **opcional**, nunca obligatorio.
- *Doble motivo:* (1) **resiliencia** — si no hay mouse o falla, la operación no se detiene; (2) **velocidad** — el flujo operativo (cobrar, etiquetar, cerrar corte) es más rápido con teclado, y compone con D17 (ruta caliente imperceptible).
- *Convenciones compartidas (DRY):* los atajos son **consistentes entre las tres caras** (etiquetado, POS, admin) para que el personal aprenda una vez; se documentan en un mapa único de atajos.
- *Kanso:* una UI enfocada y sin ruido, guiada por teclado, con **foco visible** y **orden de tabulación deliberado**, refuerza el principio de diseño y de paso da accesibilidad.
- *Alcance:* es un **principio transversal de UI**; la fundación no tiene UI, así que aquí se **fija el principio** y cada cara futura lo **realiza** (como D17 fija la meta de rendimiento y el POS la prueba). Ninguna cara debe nacer dependiente del mouse.

### D21 — Tiempo: instante en UTC capturado en el nodo; la zona local es solo para mostrar y para definir "el día"
Toda marca de tiempo SHALL guardarse como **instante UTC**, **capturado en el nodo** donde ocurre la operación —**no** lo asigna el servidor central—. La capa de almacenamiento o el espejo central (Supabase futuro) SHALL NO reescribir ni sustituir la marca del nodo. Para **mostrar** (bitácora, tickets, dashboard) y para definir **"el día"** de reportes, el instante se convierte a la **zona horaria de la sucursal** (identificador IANA, p. ej. `America/Mexico_City`), **nunca** con un desfase fijo hardcodeado.
- *El bug que evitamos:* el sistema anterior dejaba que el servidor (en EE. UU.) sellara la hora → el log decía 14:56 cuando eran las 10:00, y con eso nada es rastreable. Aquí el **nodo** sella el instante y la conversión a hora local es determinista.
- *Zona por sucursal:* cada sucursal lleva su **zona horaria** (default `America/Mexico_City`), por si hay expendios en husos distintos (frontera). El "día" de un corte/reporte es el **día natural local de esa sucursal**, no un día UTC — necesario para reportes por día y "horas activas" del dashboard.
- *Por qué UTC + IANA y no un offset fijo:* un `-6` hardcodeado se rompe con husos distintos y con cualquier cambio de horario; el instante UTC + zona IANA para presentar es inequívoco y reversible.
- *Compone con:* D11 (la bitácora es rastreable porque la hora mostrada coincide con la realidad local), D12 (`updated_at` es UTC del nodo, autoridad para el sync — el central no lo pisa) y D13 (la **sesión** define el día operativo del corte; el día natural local solo se usa para agregar reportes/horas del dashboard).
- *Confianza del reloj:* se confía en el reloj del nodo (idealmente NTP); un reloj desajustado da marcas desajustadas —higiene de dispositivo, no se auto-corrige al escribir— (un chequeo de deriva se puede añadir a futuro).

### D22 — Matriz también produce por pieza; el origen del barcode es un eje del producto
Además de comprar `pieza` a proveedores externos (con GTIN de fábrica) y producir `peso_variable`, **matriz produce productos por pieza** —incluidos los de **peso fijo** (preempacados)—. Como matriz es el productor, **acuña ella misma el barcode**: no hay proveedor que se lo dé.
- **Dos ejes ortogonales:** (1) *cómo se cobra* — `peso_variable` (precio/kg × peso real) vs `pieza` (precio unitario fijo); (2) *origen del barcode* — `externo` (GTIN de fábrica, comprados) vs `matriz` (generado, producidos). El tipo ya **no** implica el origen (antes `pieza`=externo; eso era demasiado estrecho).
- **Peso fijo = `pieza` producida por matriz** con **peso de empaque informativo**: se cobra **por unidad a precio fijo**; el peso no interviene en el cobro. No es un tipo de venta aparte — con dos tipos (`peso_variable`/`pieza`) basta (KISS).
- **Generador de barcodes en la fundación:** matriz genera **un** EAN-13 decodable **por producto (SKU)** —con dígito verificador y prefijo interno que no colisiona con GTIN externos ni con otros productos— al crear el producto. **Todas las unidades del producto comparten ese mismo barcode**, impreso en el empaque por matriz, **igual que un producto comercial** (p. ej. Bachoco); **no** es un código por-ítem. La unicidad global (spec 4.3) aplica venga externo o generado.
- **Se difiere a etiquetado (futuro):** la **impresión** física de la etiqueta, y la **etiqueta de báscula** de `peso_variable` (que codifica el peso **por ítem** al pesar, con su **discriminador antiduplicado**) — esa es otra clase de código, per-ítem, no el barcode estable de producto.
- *Compone con:* D8 (el barcode de `peso_variable` carga solo peso; el de `pieza` es un código estable de producto — no se contradicen) y la estrategia offline (el barcode se decodifica en local contra el catálogo replicado).

## Risks / Trade-offs

- **Filtrado por alcance omitido en alguna consulta** → fuga de datos entre sucursales. *Mitigación:* un único punto de acceso a datos que exige el contexto de acceso; pruebas que verifican aislamiento por sucursal.
- **Sobre-ingeniería del subsistema de autorizaciones** → *Mitigación:* **no se generaliza**. La aprobación es **concreta para gastos** (único consumidor real); no hay un segundo flujo de aprobación —el traspaso es un *envío* y el pedido es un *surtido*, ninguno es `pendiente→aprobada|rechazada`—. La máquina de estados queda como pieza interna reutilizable, y solo se generalizaría si algún día aparece un flujo de aprobación de verdad (D4).
- **Modelar el corte de caja antes de que exista el POS** → las "entradas de efectivo" no tienen fuente aún. *Mitigación:* el corte se define sobre una entrada abstracta que ya lleva su **método de pago** (D10); el POS la alimentará después sin cambiar la regla.
- **Elegir SQLite ahora condiciona el sync futuro** → *Mitigación:* mantener el dominio agnóstico de almacenamiento (repositorios como traits) para poder ajustar la estrategia de sync sin reescribir reglas.
- **Windows + Tauri usa WebView2** → PCs pre-Windows-10 requieren el runtime. *Mitigación:* documentar el requisito; reevaluar UI nativa (Slint) solo si aparece hardware muy antiguo.

## Open Questions

- **Selección de nivel en caja** (diferido al POS): qué nivel aplica según cantidad/peso, y **dónde viven los umbrales** (por producto / sucursal / global), se deciden al planear el POS. El modelo de tres niveles ya lo soporta; default propuesto → `mayoreo` = venta por caja/granel, `menudeo ↔ medio_mayoreo` por umbral automático con override si el usuario tiene el permiso.
