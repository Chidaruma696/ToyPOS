# pricing Specification

## Purpose

Define cómo ToyPOS asigna precios a los productos: el precio es función de `producto × sucursal × nivel` (`menudeo`, `medio_mayoreo`, `mayoreo`). Los fija **directamente el administrador** (con permiso `editar_precio`, sin pasar por autorización) y pueden **copiarse** de una sucursal a otra para arrancar una sucursal nueva sin recaptura. Un producto **sin precio vigente** en su sucursal queda **congelado** (existe en inventario pero no se vende). El precio se resuelve en el punto de venta desde la configuración de la sucursal y **nunca** se materializa en la etiqueta.

## Requirements

### Requirement: Precio por producto, sucursal y nivel
El sistema SHALL almacenar precios como función de `producto × sucursal × nivel`, donde nivel es uno de `menudeo`, `medio_mayoreo` o `mayoreo`. Cada combinación SHALL tener a lo sumo un precio vigente.

#### Scenario: Precios distintos por sucursal
- **WHEN** "huevo" tiene precio de menudeo 40.00 en Sucursal A y 35.00 en Sucursal B
- **THEN** cada sucursal aplica su propio precio de menudeo sin afectar a la otra

#### Scenario: Tres niveles por producto y sucursal
- **WHEN** se consulta el precio de "huevo" en Sucursal A
- **THEN** el sistema devuelve los niveles menudeo, medio_mayoreo y mayoreo definidos para esa sucursal

### Requirement: Los precios los fija el administrador de forma directa
El sistema SHALL requerir el permiso `editar_precio` para crear o modificar precios. La edición de precios SHALL ser directa y SHALL NO pasar por el subsistema de autorización.

#### Scenario: Edición sin permiso
- **WHEN** un usuario sin `editar_precio` intenta cambiar un precio
- **THEN** el sistema rechaza la operación

#### Scenario: Edición directa aplica de inmediato
- **WHEN** un usuario con `editar_precio` fija el precio de menudeo de "huevo" en Sucursal B a 35.00
- **THEN** el precio queda vigente de inmediato sin requerir aprobación de nadie

### Requirement: Copiar precios entre sucursales
El sistema SHALL permitir copiar los precios (los tres niveles) de un conjunto de productos desde una sucursal origen a una sucursal destino, para sembrar precios sin recaptura manual. La copia SHALL crear o actualizar precios en el destino sin modificar el origen.

#### Scenario: Sembrar una sucursal nueva
- **WHEN** se copian los precios de 70 productos de Sucursal A a la nueva Sucursal B
- **THEN** Sucursal B queda con los tres niveles de esos 70 productos, iguales a los de A, y Sucursal A no cambia

#### Scenario: Edición posterior es independiente
- **WHEN** tras copiar, se edita el precio de "huevo" en Sucursal B a 35.00
- **THEN** el precio de "huevo" en Sucursal A permanece en 40.00

### Requirement: Producto sin precio queda congelado
El sistema SHALL considerar un producto **congelado** (no vendible) en una sucursal cuando carezca de precio vigente para el nivel que la venta requiere; como mínimo, un producto sin precio de `menudeo` vigente en esa sucursal SHALL estar congelado. Un producto congelado SHALL poder existir en inventario pero SHALL NO poder venderse.

#### Scenario: Recibido pero sin precio
- **WHEN** un producto entra al inventario de una sucursal sin precio vigente en esa sucursal
- **THEN** el producto aparece como congelado y el sistema impide cobrarlo

#### Scenario: Descongelar al fijar precio
- **WHEN** el admin fija el precio faltante del producto congelado
- **THEN** el producto pasa a vendible sin ningún paso adicional

### Requirement: El precio no se materializa en la etiqueta
El sistema SHALL resolver el precio en el punto de venta a partir de la configuración de la sucursal; el precio SHALL NO imprimirse ni codificarse en la etiqueta del producto.

#### Scenario: Cambiar precio no invalida etiquetas
- **WHEN** el admin cambia el precio de un producto en una sucursal
- **THEN** las etiquetas ya impresas siguen siendo válidas y el nuevo precio aplica en la siguiente venta
