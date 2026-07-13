## MODIFIED Requirements

### Requirement: Catálogo de productos global gobernado por matriz
El catálogo de productos SHALL ser **global** (único para toda la organización): la matriz da de alta los productos, que quedan disponibles para todas las sucursales. Crear o editar productos SHALL requerir el permiso de-sistema `gestionar_productos`. El **inventario** (existencias por sucursal) vive en la capacidad `inventory` y es aparte del catálogo: aquí solo vive la definición del producto, que es global; sus existencias son por sucursal.

#### Scenario: Alta de producto en el catálogo global
- **WHEN** un usuario con `gestionar_productos` da de alta un producto
- **THEN** el producto queda disponible en el catálogo para todas las sucursales

#### Scenario: Alta de producto sin permiso
- **WHEN** un usuario sin `gestionar_productos` intenta dar de alta un producto
- **THEN** el sistema rechaza la operación

#### Scenario: El alta de producto no crea existencias
- **WHEN** se da de alta un producto en el catálogo global
- **THEN** el producto existe para todas las sucursales pero su existencia en cada una es cero hasta que reciba un movimiento (ver capacidad `inventory`)
