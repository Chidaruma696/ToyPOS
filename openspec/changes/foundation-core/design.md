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

## Decisions

### D1 — Workspace: un core de dominio, tres despliegues
Un **workspace de Cargo** con un crate `domain` (entidades + reglas puras, sin I/O) del que colgarán, en cambios futuros, los binarios de etiquetado, POS y admin. La lógica "precio × cantidad", permisos y estados de solicitud vive **una sola vez** en `domain`.
- *Alternativa descartada:* tres aplicaciones independientes → reimplementarían reglas y divergirían.

### D2 — Autorización basada en permisos, no en roles fijos
La autorización se evalúa contra **permisos atómicos**; los roles son solo bundles nombrados y editables. Esto satisface el requisito de "ponerle el permiso de precios al etiquetador de forma modular" sin tocar código.
- *Alternativa descartada:* enum de roles cerrado (`Admin | Cajero | Etiquetador`) → cada cambio organizativo exigiría recompilar.

### D3 — El alcance es el mecanismo de aislamiento (defensa en la capa de datos)
Cada asignación de rol lleva un alcance (`global` | `matriz` | **conjunto de sucursales**). El **alcance efectivo** del usuario es la unión de los alcances de sus asignaciones: un empleado puede estar asignado a una sola sucursal, a varias, o (admin) a todas. Toda consulta se filtra por ese alcance efectivo **en la capa de acceso a datos**, no en la UI. Se pasará un "contexto de acceso" (actor + permisos + alcance efectivo) a cada operación de repositorio, que rechaza o filtra por defecto; ningún usuario alcanza una sucursal no asignada, y dentro de la asignada solo ve lo que sus permisos autorizan.
- *Alternativa descartada:* filtrar en la UI/handlers → una ruta olvidada = fuga entre sucursales.
- *Alternativa descartada:* alcance de una sola sucursal por asignación → no cubre al empleado que trabaja en varias.

### D4 — Subsistema de autorizaciones genérico y parametrizado por tipo
Una sola entidad `SolicitudAutorizacion { tipo, solicitante, alcance, payload, estado, resolutor, resuelto_en }` con máquina de estados `pendiente → aprobada|rechazada`. El **efecto** de aprobar se delega al consumidor del tipo (gasto ahora; traspaso después) vía un contrato (trait) que el subsistema invoca sin conocer la lógica interna.
- *Alternativa descartada:* un flujo de aprobación por feature → tres implementaciones divergentes de lo mismo.
- *Nota:* los **precios NO** usan este subsistema (edición directa del admin).

### D5 — Precio = clave (producto, sucursal, nivel); "sin precio" es un estado derivado
El precio es una tabla con clave `(producto, sucursal, nivel)`. "Congelado" **no** es un flag persistido: es un estado **derivado** de la ausencia de precio de menudeo vigente en esa sucursal. Copiar precios es una operación masiva que crea/actualiza filas del destino sin tocar el origen.
- *Alternativa descartada:* un flag `congelado` persistido → se desincroniza del precio real.

### D6 — Dinero como enteros (menores), nunca float
Montos y precios se representan como **enteros en la unidad menor** (centavos) y pesos como **gramos enteros**, para evitar errores de redondeo en cortes de caja y cobros por peso.
- *Alternativa descartada:* `f64` → imprecisión inaceptable en dinero.

### D7 — Persistencia local embebida (SQLite), alineada con la ruta caliente local
La verdad operativa de cada nodo vive en **SQLite** embebido (vía `sqlx` o `rusqlite`), coherente con "la ruta caliente resuelve local". El motor de sincronización hacia la matriz es un **cambio futuro**; esta fundación deja el modelo de datos y los repositorios listos, pero no implementa sync.
- *Alternativa descartada ahora:* Postgres central como única verdad → rompe la degradación offline.

## Risks / Trade-offs

- **Filtrado por alcance omitido en alguna consulta** → fuga de datos entre sucursales. *Mitigación:* un único punto de acceso a datos que exige el contexto de acceso; pruebas que verifican aislamiento por sucursal.
- **Sobre-ingeniería del subsistema de autorizaciones con un solo consumidor (gastos)** → *Mitigación:* el segundo consumidor (traspasos) ya está previsto; el contrato se mantiene mínimo (solo estados + efecto delegado).
- **Modelar el corte de caja antes de que exista el POS** → las "entradas de efectivo" no tienen fuente aún. *Mitigación:* el corte se define sobre una entrada abstracta; el POS la alimentará después sin cambiar la regla.
- **Elegir SQLite ahora condiciona el sync futuro** → *Mitigación:* mantener el dominio agnóstico de almacenamiento (repositorios como traits) para poder ajustar la estrategia de sync sin reescribir reglas.
- **Windows + Tauri usa WebView2** → PCs pre-Windows-10 requieren el runtime. *Mitigación:* documentar el requisito; reevaluar UI nativa (Slint) solo si aparece hardware muy antiguo.

## Open Questions

- **Selección de nivel en caja** (a confirmar): default propuesto → `mayoreo` = venta por caja/granel; `menudeo ↔ medio_mayoreo` por umbral automático de cantidad/peso, con override manual si el usuario tiene permiso. La regla concreta se afinará al implementar el POS, pero el modelo de tres niveles ya la soporta.
- **Ventana de gracia de gastos pendientes**: default = hasta el corte del día siguiente. ¿Configurable por sucursal o global?
- **Autenticación**: alcance de credenciales (¿PIN de cajero vs. usuario/contraseña de admin?) — se decidirá al detallar la cara correspondiente; la fundación solo exige que exista sesión con permisos efectivos.
