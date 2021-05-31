use mcprogedit::block::Block;
use mcprogedit::block::Flower;

#[derive(Clone)]
pub struct BlockPalette {
    pub city_wall_coronation: Block,
    pub city_wall_main: Block,
    pub city_wall_top: Block,
    pub flat_window: Block,
    pub floor: Block,
    pub flowers: Vec<Flower>,
    pub foundation: Block,
    pub roof: Block,
    pub wall: Block,
}

impl Default for BlockPalette {
    fn default() -> Self {
        Self {
            city_wall_coronation: Block::Cobblestone,
            city_wall_main: Block::StoneBricks,
            city_wall_top: Block::StoneBricks,
            flat_window: Block::glass_pane(),
            floor: Block::dark_oak_planks(),
            flowers: Vec::new(),
            foundation: Block::StoneBricks,
            roof: Block::BrickBlock,
            wall: Block::Cobblestone,
        }
    }
}
