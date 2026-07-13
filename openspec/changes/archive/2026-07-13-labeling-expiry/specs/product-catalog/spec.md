## ADDED Requirements

### Requirement: Vida útil por producto
Cada producto SHALL tener una **vida útil** expresada en **meses enteros**, con default de **9 meses** cuando no se captura. Editarla SHALL requerir el permiso de-sistema `gestionar_productos` y SHALL aceptar solo enteros positivos. La vida útil determina la caducidad calculada al etiquetar (capacidad `labeling`); cambiarla SHALL NO afectar etiquetas ya emitidas.

#### Scenario: Alta sin capturar vida útil usa el default
- **WHEN** se da de alta un producto sin indicar vida útil
- **THEN** el producto queda con vida útil de 9 meses

#### Scenario: Edición de vida útil con permiso
- **WHEN** un usuario con `gestionar_productos` cambia la vida útil de un producto a 6 meses
- **THEN** el producto queda con vida útil de 6 meses para los etiquetados futuros

#### Scenario: Vida útil inválida se rechaza
- **WHEN** se intenta fijar una vida útil de 0 o negativa
- **THEN** el sistema rechaza la operación
