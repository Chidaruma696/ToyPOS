## 1. Catálogo: vida útil por producto (D33)

- [x] 1.1 `domain/producto.rs`: campo `vida_util` en meses enteros positivos con default 9; validación al alta y edición
- [x] 1.2 `storage`: columna `vida_util` con `DEFAULT 9` (migración idempotente) y edición vía repo exigiendo `gestionar_productos`
- [x] 1.3 Pruebas: default al alta, edición con/sin permiso, rechazo de 0/negativos

## 2. Dominio: etiqueta por-ítem, caducidad y cajas (D31, D32, D33, D35)

- [x] 2.1 `barcode.rs`: prefijo per-ítem reservado (distinto de `PREFIJO_MATRIZ`) y generación EAN-13 con cuerpo discriminador (5) + peso en gramos (5); rechazo de peso 0 o > 99 999 g
- [x] 2.2 `etiqueta.rs`: entidad `Etiqueta` (producto, peso, fecha_etiquetado, caducidad, sucursal, estado) con caducidad = fecha_etiquetado + vida_util en meses calendáricos, clamp a fin de mes, "el día" en la zona de la sucursal (D21)
- [x] 2.3 Contenido imprimible de exactamente 3 elementos (barcode, nombre ≤ 2 líneas, caducidad `DD/MM/AAAA`); sin precio (D8) ni fecha_etiquetado
- [x] 2.4 Entidad `Caja` que agrupa etiquetas por-ítem (`peso_variable`) o cantidad (`pieza`); caducidad impresa solo para origen matriz
- [x] 2.5 Pruebas de dominio: discriminadores distintos a peso igual, EAN-13 per-ítem válido y con peso recuperable, clamp de fin de mes y bisiesto, contenido de 3 elementos, reglas de `pieza`

## 3. Dominio: notificaciones y permisos nuevos (D36, D37)

- [x] 3.1 `notificacion.rs`: entidad con tipo (enum extensible; hoy `por_vencer`), mensaje, sucursal destino, creado, `leida_en`
- [x] 3.2 `acceso.rs`: permisos por-sucursal `etiquetar` y `ver_notificaciones` (incluidos en el contexto `sistema`)

## 4. Storage: esquema, repos y barrido (D31, D34, D36, D37)

- [x] 4.1 Migraciones idempotentes: tablas `etiqueta`, `caja_etiquetado`, `notificacion` y la secuencia del discriminador
- [x] 4.2 Repo de etiquetado: etiquetar pesadas (secuencia módulo 10⁵ con reintento ante colisión con etiqueta activa), cerrar caja, lookup por código; exige `etiquetar` + alcance; auditado (D11); sin tocar existencias (D34)
- [x] 4.3 Repo de notificaciones: emitir (solo actor `sistema`, auditado), bandeja por sucursal con `ver_notificaciones` + alcance, marcar leída idempotente que conserva la fila
- [x] 4.4 Barrido "por vencer": ventana ≤ 5 días en la zona de la sucursal, solo sucursales `expendio`, agrupado por producto × sucursal con cantidad total, emite a la sucursal afectada y a la matriz, idempotente mientras la notificación siga vigente
- [x] 4.5 Pruebas de integración: identidad por etiqueta, etiquetar no altera inventario, permisos/alcance en etiquetado y bandeja, alerta (siembra de etiquetas en expendio: agrupación, doble destino, no-duplicación, matriz excluida, fuera de ventana)

## 5. Verificación

- [x] 5.1 `cargo fmt --check`, `cargo clippy` y `cargo test --workspace` en verde
- [x] 5.2 `openspec validate --all` en verde y tasks.md al día
