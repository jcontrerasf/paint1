# paint1

Editor de imágenes tipo Paint en Rust; todo lo dibujado son objetos editables.

## Convenciones

- Código, identificadores y comentarios en **inglés**. Los textos visibles van **solo** en
  `src/i18n.rs` (español e inglés; un texto que falta en un idioma no compila).
- Sin emojis en la UI: íconos de Phosphor en `src/icons.rs` (códigos de egui-phosphor 0.14).
- Colores solo desde `theme::pal()` (paletas `LIGHT`/`DARK` en `src/theme.rs`).
- Distribución como EdenBord: barra superior, barra de contexto, herramientas a la izquierda,
  panel derecho (objetos + propiedades), barra de estado. Las barras reservan su lado derecho
  primero con `right_to_left` y recortan lo que no cabe.
- egui/eframe 0.37: `egui::Panel::top/left/...show(ui, ..)`, `App::ui` + `App::logic`.
- Antes de terminar: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

## Arquitectura

- `document.rs`: modelo (`Document`, `Object`, `Kind`). Geometría en coordenadas locales +
  `transform` (kurbo `Affine`); `Object::normalize` pasa la escala a la geometría cuando es
  exacto, así escalar no engrosa el borde.
- `render.rs`: único renderizador (tiny-skia). El lienzo en pantalla y la exportación usan el
  mismo código. La goma usa `DestinationOut` en una capa aparte para no borrar el fondo.
- `history.rs`: `Session` con deshacer por instantáneas; los cambios continuos usan
  `edit_grouped(clave)` y se cierran en `logic()` cuando no hay botón apretado ni foco.
- `tools/`: herramientas del lienzo; `ui/`: paneles y diálogos; `canvas.rs`: vista y textura.
- `raster.rs`: operaciones de píxeles (`flatten`, `lift` para selección rectangular/libre).
  El fondo del lienzo es siempre un color sólido; no hay capa raster de fondo.
- `file_format.rs` (`.paint1`), `export.rs` (formatos y límite de tamaño), `clipboard.rs`.
