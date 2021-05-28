use crate::build_area::BuildArea;
use mcprogedit::block::Block;
use mcprogedit::coordinates::BlockCoord;
use mcprogedit::world_excerpt::WorldExcerpt;

pub fn build_rock(excerpt: &WorldExcerpt, build_area: &BuildArea) -> WorldExcerpt {
    let height_map = excerpt.height_map();

    let (x_len, y_len, z_len) = excerpt.dim();
    let mut output = WorldExcerpt::new(x_len, y_len, z_len);

    for x in 0..x_len {
        for z in 0..z_len {
            if let Some(designation) = build_area.designation_at((x as usize, z as usize)) {
                if designation.is_buildable() {
                    // Get terrain height
                    if let Some(y) = height_map.height_at((x as usize, z as usize)) {
                        // Build something on the terrain
                        for y in y..y+5 {
                            output.set_block_at(
                                BlockCoord(x as i64, y as i64, z as i64),
                                Block::Stone,
                            );
                        }
                    }
                }
            }
        }
    }

    output
}
