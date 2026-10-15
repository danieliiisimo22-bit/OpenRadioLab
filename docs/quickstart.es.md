# Open Radio Lab — guía rápida

Aplicación nativa en Rust y Slint para inspeccionar, editar y programar radios compatibles. El perfil actual corresponde a TK-2402K; el nombre genérico del proyecto no significa compatibilidad universal.

## Lectura directa

Entre en **Conexión**, seleccione el puerto de la radio y pulse **Leer configuración de la radio**. No necesita abrir un DAT. Al terminar se cargan los canales y se guarda un respaldo completo.

## Edición y escritura

Seleccione un canal, cambie los valores y pulse **Aplicar canal**. En **Archivos → Guardar como…** puede guardar una copia con el explorador. En **Conexión → Escribir cambios en la radio** puede programar los campos admitidos.

Antes de escribir, el programa lee y respalda la radio. Después escribe solo bloques modificados, abre una conexión nueva y compara los 8192 bytes con el plan. Un error detiene la operación sin reintentar o restaurar automáticamente. Cierre otras aplicaciones de programación mientras usa el puerto.

Los datos privados se guardan en `open-radio-lab-data/`, junto al ejecutable. El argumento `--data-dir` permite escoger otra ubicación. Estos datos no forman parte del repositorio público.

## Alcance

Se han ejercitado identificación, lectura y escritura de Scan Add CH1 con verificación completa. También se reportaron cambios físicos correctos de RX/TX y QT Encode/Decode. None/DQT se representan en JSON/CSV, pero su edición DAT todavía está bloqueada.

## Código abierto

La licencia es GPL-3.0-only. El código usa identificadores y comentarios en inglés; la interfaz actual está en español. Consulte el [inventario de lenguajes](source-languages.md): porcentajes reales por bytes de código propio, con metodología y listado de archivos. No representan el porcentaje de funciones terminadas.

Para contribuir, lea [CONTRIBUTING.md](../CONTRIBUTING.md). Para compilar, vea [build.md](build.md).

