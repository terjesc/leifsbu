use crate::geometry;
use crate::geometry::{manhattan_distance, LandUsageGraph, RawEdge};
use mcprogedit::coordinates::{BlockColumnCoord, BlockCoord};

#[derive(Clone, Debug)]
pub struct Plot {
    pub edges: Vec<PlotEdge>,
}

#[derive(Clone, Debug)]
pub struct PlotEdge {
    pub kind: PlotEdgeKind,
    pub points: Vec<BlockCoord>,
}

#[derive(Clone, Debug)]
pub enum PlotEdgeKind {
    Road { width: usize },
    Wall { width: usize },
    Plot,
    Terrain,
}

impl Plot {
    pub fn polygon(&self) -> Vec<BlockColumnCoord> {
        let mut polygon = Vec::new();

        for edge in &self.edges {
            for point in &edge.points {
                polygon.push(BlockColumnCoord::from(*point));
            }
        }

        polygon
    }

    pub fn point_slice(&self) -> Vec<imageproc::point::Point<i64>> {
        let point_vec: Vec<imageproc::point::Point<i64>> = self
            .polygon()
            .iter()
            .map(|coordinates| imageproc::point::Point::<i64>::new(coordinates.0, coordinates.1))
            .collect();

        point_vec
    }

    pub fn split(&self, split_line: &RawEdge) -> (Plot, Plot) {
        // TODO Do some geometry and stuff:
        // 1) Add all intersection points between `self` and `split_line` to `self`
        // 2) Go through all points, put them in `plot_0` / `plot_1` depending
        // on what side of line they are on (or both if it is a split point.)
        //
        // TODO Then return `(plot_0, plot_1)`
        todo!()
    }

    pub fn has_access(&self) -> bool {
        for edge in &self.edges {
            match edge.kind {
                PlotEdgeKind::Road { .. } => return true,
                _ => (),
            }
        }
        false
    }
}

pub fn divide_city_block(
    city_block: &Vec<BlockColumnCoord>,
    land_usage: &LandUsageGraph,
) -> Vec<Plot> {
    let plot = land_usage.plot_from_area(city_block);
    divide_plot(&plot)
}

fn divide_plot(plot: &Plot) -> Vec<Plot> {
    rec_subdiv_obb(plot, (40, 120))
}

fn rec_subdiv_obb(plot: &Plot, area_bounds: (i64, i64)) -> Vec<Plot> {
    let polygon = plot.polygon();
    let area = geometry::area(&polygon);

    // Do not split if already small enough
    if area < area_bounds.1 {
        return vec![plot.clone()];
    }

    // NB May add front side width constraint, similar to area constraint above.

    // Get potential split lines
    let (short_edge, long_edge) = compute_split_lines(plot);

    // Split the plot
    let (plot_1, plot_2) = {
        let (short_plot_1, short_plot_2) = plot.split(&short_edge);
        if short_plot_1.has_access() && short_plot_2.has_access() {
            (short_plot_1, short_plot_2)
        } else {
            let (long_plot_1, long_plot_2) = plot.split(&long_edge);
            (long_plot_1, long_plot_2)
        }
    };

    // Build the output from recurring on the two plots from the split
    let mut plots = rec_subdiv_obb(&plot_1, area_bounds);
    plots.append(&mut rec_subdiv_obb(&plot_2, area_bounds));

    plots
}

/// Find potential split lines for the plot, from the Oriented Bounding Box (OBB).
fn compute_split_lines(plot: &Plot) -> (RawEdge, RawEdge) {
    let point_slice = plot.point_slice();
    let obb = imageproc::geometry::min_area_rect(&point_slice);

    let (p0, p1, p2, p3) = (obb[0], obb[1], obb[2], obb[3]);

    let split_line_0 = (
        (BlockColumnCoord(p0.x, p0.y) + BlockColumnCoord(p1.x, p1.y)) / 2,
        (BlockColumnCoord(p2.x, p2.y) + BlockColumnCoord(p3.x, p3.y)) / 2,
    );
    let split_line_1 = (
        (BlockColumnCoord(p1.x, p1.y) + BlockColumnCoord(p2.x, p2.y)) / 2,
        (BlockColumnCoord(p3.x, p3.y) + BlockColumnCoord(p0.x, p0.y)) / 2,
    );

    // Figure out which one is the short one and which one is the long one.
    let len_0 = manhattan_distance(split_line_0.0, split_line_0.1);
    let len_1 = manhattan_distance(split_line_1.0, split_line_1.1);

    // Return the short one first
    if len_0 < len_1 {
        (split_line_0, split_line_1)
    } else {
        (split_line_1, split_line_0)
    }
}
