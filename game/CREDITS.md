# Power Grid: art credits

## Voxel art (current)
Every sprite the game ships (terrain, trees, buildings, people, the delivery truck) is an original voxel model in
`tools/voxel/models.py`, rendered by `tools/voxel/vox.py`. No third-party art is in `web/atlas.png`.

## Earlier pixel art (no longer shipped)
The sections below credit the art used up to build .34. Its source sheets and the old builder (`tools/build_atlas.py`)
are kept for reference only.

## OpenGFX (GPL-2.0)
Terrain, trees, power-station, gas-plant and building sprites in `../assets/sprites/opengfx/` and baked into `web/atlas.png`
come from **OpenGFX**, the open graphics base set for OpenTTD, by the OpenGFX team and contributors.
- Source: https://github.com/OpenTTD/OpenGFX
- License: GNU General Public License v2 (see `LICENSE-OpenGFX-GPL-2.0.txt`)
- Changes: individual sprites were cut out of the sheets and the blue key colour made transparent.

If you ship the game with these sprites, ship them under GPL-2.0 with this credit and make
their source (the original sheets, or this folder) available alongside.

## Original Power Grid sprites
Miner containers, immersion tanks, transformers, fan walls, diesel gensets, solar panels,
wind turbines, the dam, the reactor dome, the fusion tokamak and the free energy core are
drawn by code in `tools/ttdmap.py` for this project.

## Site ground textures (CC0)
The ground for Iceland, Paraguay, Ethiopia, Bhutan and North Dakota is made from **rubberduck**'s
isometric ground tiles and grassland tileset on OpenGameArt, both released as CC0 (public domain, no credit required).
- https://opengameart.org/content/isometric-ground-tiles
- https://opengameart.org/content/grassland-tileset-1
- Source sheets are in `../assets/sprites/rubberduck/`.
- Changes: `tools/site_ground.py` cuts the plain tiles to the game's 64x31 diamond, wraps them seamlessly,
  boosts local contrast and remaps them onto per-site colour ramps with ordered dithering. Patches (snow, red earth,
  rubble) use the pack's transition shapes as blob masks.

## Renders
`renders/` are seeded map renders (seed 7F3A) and power-ladder previews made with `tools/scenes.py`.

## Also considered (not used yet)
- Kenney Isometric road assets, CC0: https://kenney.nl
