# Supplemental Lucide Icons

The application uses `gpui-component::Icon` and `IconName` with the official
`gpui-component-assets = 0.5.1` resource bundle. These files provide only
reference variants absent from that release's default icon set:

- `tag.svg`
- `circle-alert.svg`
- `files.svg`
- `video.svg`
- `image.svg`
- `layout-grid.svg`
- `list.svg`
- `rotate-ccw.svg`
- `user-round.svg`
- `clock.svg`
- `film.svg`
- `scan.svg`
- `square.svg`
- `share-2.svg`
- `copyright.svg`
- `pen-tool.svg`
- `trash-2.svg`
- `nav-menu.svg`, `nav-star.svg`, `nav-settings.svg`: sidebar-specific reference variants

Geometry was extracted without redrawing from the exported `__iconData.node`
values in `@lucide/vue` 1.52.0, the exact version in the reference
`package-lock.json`. The downloaded package's SHA-512 integrity was checked
against that lockfile. Vue-only node keys were omitted; 24x24 viewBox,
currentColor, 2-pixel stroke, round caps/joins and all original geometric
attributes are preserved. See [the upstream license](../../LICENSE-icons).

The Rust asset provider embeds these files and delegates all other resource
paths to `gpui-component-assets`. Its tests parse and rasterize both sets with
resvg 0.45.1, the renderer version already used by the pinned GPUI release,
and assert nonempty alpha coverage. There are no runtime icon downloads or
frontend/package-install requirements.
