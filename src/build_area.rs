use crate::geometry;
use crate::line;
use crate::plot::{Plot, PlotEdgeKind};
use mcprogedit::coordinates::BlockColumnCoord;
use mcprogedit::world_excerpt::WorldExcerpt;

/// What land use a block (or a column of blocks) is intended for.
#[derive(Clone, Copy, Debug)]
pub enum AreaDesignation {
    None,
    Irrelevant(BuildRights),
    Plot(BuildRights),
    Road(BuildRights),
    Wall(BuildRights),
}

impl AreaDesignation {
    /// True if all blocks covered by this designation can be modified.
    pub fn is_buildable(&self) -> bool {
        match self {
            Self::Irrelevant(BuildRights::Buildable)
            | Self::Plot(BuildRights::Buildable)
            | Self::Road(BuildRights::Buildable)
            | Self::Wall(BuildRights::Buildable) => true,
            _ => false,
        }
    }

    /// True if air blocks covered by this designation can be modified,
    /// regardless of whether or not other kinds of blocks can be modified.
    pub fn is_air_buildable(&self) -> bool {
        match self {
            Self::Irrelevant(BuildRights::AirBuildable)
            | Self::Plot(BuildRights::AirBuildable)
            | Self::Road(BuildRights::AirBuildable)
            | Self::Wall(BuildRights::AirBuildable) => true,
            _ => self.is_buildable(),
        }
    }

    /// True if modification is not allowed for any blocks covered by this designation.
    pub fn is_forbidden(&self) -> bool {
        match self {
            Self::Irrelevant(BuildRights::Forbidden)
            | Self::Plot(BuildRights::Forbidden)
            | Self::Road(BuildRights::Forbidden)
            | Self::Wall(BuildRights::Forbidden) => true,
            _ => false,
        }
    }
}

/// What changes are allowed for a block or a column of blocks.
#[derive(Clone, Copy, Debug)]
pub enum BuildRights {
    /// Full rights to modifying any blocks
    Buildable,
    /// Can replace air blocks
    AirBuildable,
    /// No rights to modify any blocks
    Forbidden,
}

/// 2D area usage plan and access rights.
pub struct BuildArea {
    designations: Vec<AreaDesignation>,
    x_dim: usize,
    z_dim: usize,
}

impl BuildArea {
    /// Returns a new BuildArea of the given dimensions, with all designations unset.
    pub fn new((x_dim, z_dim): (usize, usize)) -> Self {
        Self::new_with_designation((x_dim, z_dim), AreaDesignation::None)
    }

    /// Returns a new BuildArea of the given dimensions, with all designations set to `designation`.
    pub fn new_with_designation(
        (x_dim, z_dim): (usize, usize),
        designation: AreaDesignation,
    ) -> Self {
        let designations_len = x_dim * z_dim;
        let designations = vec![designation; designations_len];
        Self {
            designations,
            x_dim,
            z_dim,
        }
    }

    /// Generate a BuildArea for the given WorldExcerpt and Plot
    pub fn from_world_excerpt_and_plot(excerpt: &WorldExcerpt, plot: &Plot) -> Self {
        let (x_len, _, z_len) = excerpt.dim();
        let plot_polygon = plot.polygon();

        // Unless any other information exists, the area is forbidden and of irrelevant type.
        let mut build_area = Self::new_with_designation(
            (x_len, z_len),
            AreaDesignation::Irrelevant(BuildRights::Forbidden),
        );

        // Fill the inside of the plot as buildable plot.
        for x in 0..x_len {
            for z in 0..z_len {
                if geometry::InOutSide::Inside == geometry::point_position_relative_to_polygon(
                    BlockColumnCoord(x as i64, z as i64),
                    &plot_polygon,
                ) {
                    build_area.set_designation_at(
                        (x, z),
                        AreaDesignation::Plot(BuildRights::Buildable),
                    );
                }
            }
        }

        // Designate the areas immediately surrounding the plot
        for edge in &plot.edges {
            match edge.kind {
                PlotEdgeKind::Road { width } => {
                    let line = line::line(&edge.points.0, &edge.points.1, width as i64);

                    for position in &line {
                        let coordinates = (position.0 as usize, position.2 as usize);
                        build_area.set_designation_at(
                            coordinates,
                            AreaDesignation::Road(BuildRights::Forbidden),
                        );
                    }
                }
                PlotEdgeKind::Wall { width } => {
                    let line = line::line(&edge.points.0, &edge.points.1, width as i64);

                    for position in &line {
                        let coordinates = (position.0 as usize, position.2 as usize);
                        if let Some(AreaDesignation::Road(_)) =
                            build_area.designation_at(coordinates)
                        {
                            // Do not overwrite roads with wall.
                        } else {
                            build_area.set_designation_at(
                                coordinates,
                                AreaDesignation::Wall(BuildRights::Forbidden),
                            );
                        }
                    }
                }
                PlotEdgeKind::Plot => {
                    let line = line::line(&edge.points.0, &edge.points.1, 2i64);

                    for position in &line {
                        let coordinates = (position.0 as usize, position.2 as usize);
                        if let Some(AreaDesignation::Irrelevant(_)) =
                            build_area.designation_at(coordinates)
                        {
                            build_area.set_designation_at(
                                coordinates,
                                AreaDesignation::Plot(BuildRights::Forbidden),
                            );
                        }
                    }
                }
                PlotEdgeKind::Terrain => (),
            }
        }

        // TODO Road neighbouring Buildable Plot should be AirBuildable.
        // it would allow e.g putting down stairs, flower boxes, torches, roof overhangs, etc.

        build_area
    }

    /// Get the dimensions of this BuildArea, as `(x_dimension, z_dimension)`.
    pub fn dimensions(&self) -> (usize, usize) {
        (self.x_dim, self.z_dim)
    }

    /// Set the designation at the (x, z) location `coordinates` to the given designation.
    pub fn set_designation_at(&mut self, coordinates: (usize, usize), designation: AreaDesignation) {
        if let Some(index) = self.index(coordinates) {
            self.designations[index] = designation;
        }
    }

    /// Get the designation at the (x, z) location `coordinates`.
    pub fn designation_at(&self, coordinates: (usize, usize)) -> Option<AreaDesignation> {
        if let Some(index) = self.index(coordinates) {
            Some(*self.designations.get(index).unwrap())
        } else {
            None
        }
    }

    fn index(&self, (x, z): (usize, usize)) -> Option<usize> {
        if x >= self.x_dim || z >= self.z_dim {
            None
        } else {
            Some(x + self.x_dim * z)
        }
    }
}
