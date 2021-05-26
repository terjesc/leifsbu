use crate::pathfinding::{RoadNode, RoadPath};
use crate::plot::{Plot, PlotEdge, PlotEdgeKind};
use crate::types::Snake;
use image::GrayImage;
use mcprogedit::coordinates::{BlockColumnCoord, BlockCoord};
use std::cmp::{max, min};
use std::collections::{HashMap, HashSet, VecDeque};
use std::f32::consts::PI;

pub type RawEdge = (BlockColumnCoord, BlockColumnCoord);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeftRightSide {
    Left,
    On,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InOutSide {
    Inside,
    _On,
    Outside,
}

/// For a line through `line.0` and `line.1`, in that direction,
/// which side is `point` on relative to the line?
pub fn point_position_relative_to_line(point: BlockColumnCoord, line: RawEdge) -> LeftRightSide {
    let double_area = (line.1 .0 - line.0 .0) * (point.1 - line.0 .1)
        - (point.0 - line.0 .0) * (line.1 .1 - line.0 .1);

    if double_area > 0 {
        LeftRightSide::Left
    } else if double_area < 0 {
        LeftRightSide::Right
    } else {
        LeftRightSide::On
    }
}

/// For a (counter-clockwise) `polygon`, is `point` inside or outside of the polygon?
pub fn point_position_relative_to_polygon(
    point: BlockColumnCoord,
    polygon: &[BlockColumnCoord],
) -> InOutSide {
    let winding_number = polygon.windows(2).fold(0i64, |winding_number, line| {
        if line[0].1 <= point.1 {
            if line[1].1 > point.1
                && LeftRightSide::Left == point_position_relative_to_line(point, (line[0], line[1]))
            {
                winding_number + 1
            } else {
                winding_number
            }
        } else {
            if line[1].1 <= point.1
                && LeftRightSide::Right
                    == point_position_relative_to_line(point, (line[0], line[1]))
            {
                winding_number - 1
            } else {
                winding_number
            }
        }
    });

    if 0 == winding_number {
        InOutSide::Outside
    } else {
        InOutSide::Inside
    }
}

#[derive(Clone, Copy, Debug, Ord, PartialEq, PartialOrd, Eq)]
pub enum EdgeKind {
    Road,
    Street,
    Wall,
}

#[derive(Clone, Copy, Debug, Ord, PartialEq, PartialOrd, Eq)]
struct EdgeMeta {
    kind: EdgeKind,
    width: i64,
}

#[derive(Clone, Copy, Debug, Ord, PartialEq, PartialOrd, Eq)]
struct VertexMeta {
    access_y: Option<i64>,
}

pub struct LandUsageGraph {
    edges: HashMap<BlockColumnCoord, Vec<BlockColumnCoord>>,
    edge_meta: HashMap<RawEdge, EdgeMeta>,
    vertex_meta: HashMap<BlockColumnCoord, VertexMeta>,
}

impl LandUsageGraph {
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
            edge_meta: HashMap::new(),
            vertex_meta: HashMap::new(),
        }
    }

    pub fn plot_from_area(&self, area: &Vec<BlockColumnCoord>) -> Plot {
        let mut edges = Vec::new();

        for edge in area.windows(2) {
            match self.edge_meta.get(&(edge[0], edge[1])) {
                None => (),
                Some(EdgeMeta { kind, width }) => {
                    // NB we don't have heights for circumference here...
                    let y0 = self
                        .vertex_meta
                        .get(&edge[0])
                        .unwrap()
                        .access_y
                        .unwrap_or(0);
                    let y1 = self
                        .vertex_meta
                        .get(&edge[1])
                        .unwrap()
                        .access_y
                        .unwrap_or(0);

                    let kind = match kind {
                        EdgeKind::Road | EdgeKind::Street => PlotEdgeKind::Road {
                            width: *width as usize,
                        },
                        EdgeKind::Wall => PlotEdgeKind::Wall {
                            width: *width as usize,
                        },
                    };
                    edges.push(PlotEdge {
                        kind,
                        points: vec![
                            BlockCoord(edge[0].0, y0, edge[0].1),
                            BlockCoord(edge[1].0, y1, edge[1].1),
                        ],
                    });
                }
            }
        }

        Plot { edges }
    }

    /// Add roads to the land usage graph, of the given kind and width.
    pub fn add_roads(&mut self, roads: &Vec<RoadPath>, kind: EdgeKind, width: i64) {
        for road in roads {
            for segment in road.windows(2) {
                let p0 = segment[0].coordinates.into();
                let p1 = segment[1].coordinates.into();

                // Add edges
                self.edges.entry(p0).or_insert(Vec::new()).push(p1);
                self.edges.entry(p1).or_insert(Vec::new()).push(p0);
                let meta = EdgeMeta { kind, width };
                self.edge_meta.insert((p0, p1), meta);
                self.edge_meta.insert((p1, p0), meta);

                // Add vertices
                self.vertex_meta.insert(
                    p0,
                    VertexMeta {
                        access_y: Some(segment[0].coordinates.1),
                    },
                );
                self.vertex_meta.insert(
                    p1,
                    VertexMeta {
                        access_y: Some(segment[1].coordinates.1),
                    },
                );
            }
        }
    }

    /// Add a circumference to the graph, of the given kind and width.
    pub fn add_circumference(&mut self, circumference: &Snake, kind: EdgeKind, width: i64) {
        for segment in circumference.windows(2) {
            let p0 = segment[0];
            let p1 = segment[1];

            // Add edges
            self.edges.entry(p0).or_insert(Vec::new()).push(p1);
            self.edge_meta.insert((p0, p1), EdgeMeta { kind, width });

            // Add vertices
            self.vertex_meta.insert(p0, VertexMeta { access_y: None });
            self.vertex_meta.insert(p1, VertexMeta { access_y: None });
        }
    }

    /// Return a list of the edges in this graph structure.
    // TODO change to returning an iterator instead
    pub fn edges(&self) -> Vec<(BlockColumnCoord, BlockColumnCoord)> {
        let mut edges = Vec::new();

        for (start_point, end_points) in &self.edges {
            for end_point in end_points {
                edges.push((*start_point, *end_point));
            }
        }

        edges
    }

    /// Returns the "left-most" turn from b (not b itself), when coming from a.
    fn get_left_turn(&self, (a, b): RawEdge) -> Option<BlockColumnCoord> {
        match self.edges.get(&b) {
            None => None,
            Some(next_vertices) => {
                if next_vertices.is_empty() {
                    None
                } else {
                    //let mut best_coordinates = *next_vertices.first().unwrap();
                    let mut best_coordinates = None;
                    //let mut best_angle = Self::angle(a, b, best_coordinates);
                    let mut best_angle = f32::MIN;

                    for coordinates in next_vertices {
                        if *coordinates == b {
                            continue;
                        }

                        let angle = Self::angle(a, b, *coordinates);

                        if angle > best_angle {
                            best_angle = angle;
                            best_coordinates = Some(*coordinates);
                        }
                    }

                    best_coordinates
                }
            }
        }
    }

    fn angle(a: BlockColumnCoord, b: BlockColumnCoord, c: BlockColumnCoord) -> f32 {
        // a = atan2d(x1*y2-y1*x2,x1*x2+y1*y2);
        let (x1, y1) = (b.0 - a.0, b.1 - a.1);
        let (x2, y2) = (c.0 - b.0, c.1 - b.1);

        let angle = ((x1 * y2 - y1 * x2) as f32).atan2((x1 * x2 + y1 * y2) as f32);
        if angle == PI {
            -PI
        } else {
            angle
        }
    }
}

/// Returns a set of polygons corresponding to the areas sectioned by the structures in `graph`.
pub fn extract_blocks(graph: &LandUsageGraph) -> Vec<Vec<BlockColumnCoord>> {
    let mut queue = VecDeque::<RawEdge>::new();
    let mut visited = HashSet::<RawEdge>::new();
    let mut areas = Vec::<Vec<BlockColumnCoord>>::new();

    // Populate queue
    //println!("Populating queue…");
    for edge in graph.edges() {
        queue.push_back(edge);
    }
    //println!("Queue populated with {} edges.", queue.len());

    // For each element in queue:
    while let Some(edge) = queue.pop_front() {
        if visited.contains(&edge) {
            //println!("Already visited edge {:?}", edge);
            continue;
        } else {
            //println!("Visiting edge {:?} for the first time", edge);
            visited.insert(edge);
        }

        let first_edge = edge;

        let mut area = Vec::<BlockColumnCoord>::new();
        let mut visited_in_area = HashSet::<RawEdge>::new();

        area.push(first_edge.0);
        area.push(first_edge.1);
        visited_in_area.insert(first_edge);

        let mut current_edge = first_edge;

        loop {
            let next_vertex = match graph.get_left_turn(current_edge) {
                None => {
                    //println!("No next vertex from {:?}", current_edge);
                    break;
                }
                Some(vertex) => {
                    //println!("Next vertex from {:?} is {:?}", current_edge, vertex);
                    vertex
                }
            };

            let next_edge = (current_edge.1, next_vertex);
            visited.insert(next_edge);

            if visited_in_area.contains(&next_edge) {
                println!(
                    "We found a loop (size {}) when starting from edge {:?}, that loops from {:?}",
                    area.len(),
                    first_edge,
                    next_edge,
                );

                if first_edge == next_edge {
                    println!("The loop is accepted.");
                    areas.push(area);
                }
                break;
            }

            visited_in_area.insert(next_edge);
            area.push(next_vertex);
            current_edge = next_edge;
        }
    }
    println!("Found {} areas.", areas.len());
    areas
}

/// Add common points where roads intersect with the snake.
/// If the snake intersects a road segment multiple places, then an arbitrary
/// intersection gets selected for that intersection point.
pub fn add_intersection_points(roads: &mut Vec<RoadPath>, snake: &mut Snake) {
    // For storing intersections that should be added to the snake after roads are handled
    let mut snake_extra_points = HashMap::<RawEdge, Vec<BlockColumnCoord>>::new();

    // First, handle the roads
    let mut new_roads = Vec::new();

    for road in roads.iter() {
        let mut new_road = Vec::new();
        new_road.push(road[0]);

        for road_segment in road.windows(2) {
            for snake_segment in snake.windows(2) {
                let raw_road_segment = (
                    road_segment[0].coordinates.into(),
                    road_segment[1].coordinates.into(),
                );
                let snake_segment = (snake_segment[0], snake_segment[1]);

                match intersection(raw_road_segment, snake_segment) {
                    IntersectionPoints::None => (), // No intersection
                    IntersectionPoints::One(p) | IntersectionPoints::Two(p, _) => {
                        // NB Not using the second point for two "intersection" points.
                        //    That might happen when the lines are parallell and overlapping.
                        //    It should be fine to just use one of the points, arbitrarily,
                        //    in that situation. No need to go overboard.
                        if p == raw_road_segment.0 || p == raw_road_segment.1 {
                            // Don't add already existing points
                        } else {
                            // NB using arithmetic mean instead of proper interpolation here.
                            let y =
                                (road_segment[0].coordinates.1 + road_segment[1].coordinates.1) / 2;
                            let kind = road_segment[0].kind;
                            let coordinates = BlockCoord(p.0, y, p.1);
                            new_road.push(RoadNode { coordinates, kind });
                        }

                        if p == snake_segment.0 || p == snake_segment.1 {
                            // Don't add already existing points
                        } else {
                            snake_extra_points
                                .entry(snake_segment)
                                .or_insert(Vec::new())
                                .push(p);
                        }
                    }
                }
            }
            new_road.push(road_segment[1]);
        }
        new_roads.push(new_road);
    }

    *roads = new_roads;

    // Then, handle the snake
    let mut new_snake = Vec::new();
    new_snake.push(*snake.first().unwrap());

    for segment in snake.windows(2) {
        match snake_extra_points.get(&(segment[0], segment[1])) {
            None => (),
            Some(points) => {
                for point in points {
                    // NB pushing points in arbitrary order. They should rather get sorted
                    // according to the direction of the snake. There might be more than one
                    // road crossing a snake segment, which might lead to trouble...
                    new_snake.push(*point);
                }
            }
        }
        new_snake.push(segment[1]);
    }

    *snake = new_snake;
}

enum IntersectionPoints {
    None,
    One(BlockColumnCoord),
    Two(BlockColumnCoord, BlockColumnCoord),
}

fn intersection(edge_a: RawEdge, edge_b: RawEdge) -> IntersectionPoints {
    let (BlockColumnCoord(a_x1, a_y1), BlockColumnCoord(a_x2, a_y2)) = edge_a;
    let (BlockColumnCoord(b_x1, b_y1), BlockColumnCoord(b_x2, b_y2)) = edge_b;

    let (a1, b1) = (a_y2 - a_y1, a_x1 - a_x2);
    let c1 = a1 * a_x1 + b1 * a_y1;
    let (a2, b2) = (b_y2 - b_y1, b_x1 - b_x2);
    let c2 = a2 * b_x1 + b2 * b_y1;

    let determinant = a1 * b2 - a2 * b1;

    if determinant == 0 {
        // Lines are parallel
        let a_ratio = a1 as f32 / a2 as f32;
        let b_ratio = b1 as f32 / b2 as f32;
        let c_ratio = c1 as f32 / c2 as f32;
        if a_ratio == b_ratio && b_ratio == c_ratio {
            // Segments may overlap, as their infinite continuations are  identical
            // TODO check if they overlap.
            // There may be one or two instances of a point on one laying on the line of the other,
            // or of coinciding points.
            IntersectionPoints::None
        } else {
            // Lines are not overlapping
            IntersectionPoints::None
        }
    } else {
        let x = (b2 * c1 - b1 * c2) / determinant;
        let y = (a1 * c2 - a2 * c1) / determinant;

        if min(a_x1, a_x2) <= x
            && x <= max(a_x1, a_x2)
            && min(b_x1, b_x2) <= x
            && x <= max(b_x1, b_x2)
            && min(a_y1, a_y2) <= y
            && y <= max(a_y1, a_y2)
            && min(b_y1, b_y2) <= y
            && y <= max(b_y1, b_y2)
        {
            IntersectionPoints::One(BlockColumnCoord(x, y))
        } else {
            IntersectionPoints::None
        }
    }
}

/// Calculates the area of a polygon, using the shoelace formula
pub fn area(polygon: &[BlockColumnCoord]) -> i64 {
    if polygon.len() < 3 {
        return 0;
    }

    let additional_term = if polygon.first() != polygon.last() {
        polygon.last().unwrap().0 * polygon.first().unwrap().1
            - polygon.last().unwrap().1 * polygon.first().unwrap().0
    } else {
        0
    };

    polygon
        .windows(2)
        .fold(additional_term, |area: i64, edge: _| {
            area + (edge[0].0 * edge[1].1 - edge[0].1 * edge[1].0)
        })
        / 2
}

pub fn draw_area(
    image: &mut GrayImage,
    area: &Vec<BlockColumnCoord>,
    offset: BlockColumnCoord,
    colour: image::Luma<u8>,
) {
    let (x_len, z_len) = image.dimensions();

    for x in 0..x_len {
        for z in 0..z_len {
            if InOutSide::Inside
                == point_position_relative_to_polygon(
                    BlockColumnCoord(x as i64, z as i64) + offset,
                    &area,
                )
            {
                image.put_pixel(x, z, colour);
            }
        }
    }
}

pub fn manhattan_distance(a: BlockColumnCoord, b: BlockColumnCoord) -> usize {
    (a.0 as i64 - b.0 as i64).abs() as usize + (a.1 as i64 - b.1 as i64).abs() as usize
}

pub fn manhattan_distance_3d(a: BlockCoord, b: BlockCoord) -> usize {
    (a.0 - b.0).abs() as usize + (a.1 - b.1).abs() as usize + (a.2 - b.2).abs() as usize
}

pub fn euclidean_distance(a: BlockColumnCoord, b: BlockColumnCoord) -> f32 {
    ((a.0 as f32 - b.0 as f32).powi(2) + (a.1 as f32 - b.1 as f32).powi(2)).sqrt()
}

pub fn euclidean_distance_3d(a: BlockCoord, b: BlockCoord) -> f32 {
    ((a.0 as f32 - b.0 as f32).powi(2)
        + (a.1 as f32 - b.1 as f32).powi(2)
        + (a.2 as f32 - b.2 as f32).powi(2))
    .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::f32::consts::PI;

    #[test]
    fn point_left_of_line() {
        assert_eq!(
            LeftRightSide::Left,
            point_position_relative_to_line(
                BlockColumnCoord(-2, -2),
                (BlockColumnCoord(3, -4), BlockColumnCoord(1, 2)),
            ),
        );
    }

    #[test]
    fn point_right_of_line() {
        assert_eq!(
            LeftRightSide::Right,
            point_position_relative_to_line(
                BlockColumnCoord(4, 1),
                (BlockColumnCoord(3, -4), BlockColumnCoord(1, 2)),
            ),
        );
    }

    #[test]
    fn point_on_line() {
        assert_eq!(
            LeftRightSide::On,
            point_position_relative_to_line(
                BlockColumnCoord(2, -1),
                (BlockColumnCoord(3, -4), BlockColumnCoord(1, 2)),
            ),
        );
    }

    #[test]
    fn point_inside_polygon() {
        assert_eq!(
            InOutSide::Inside,
            point_position_relative_to_polygon(
                BlockColumnCoord(2, -1),
                &[
                    BlockColumnCoord(3, -4),
                    BlockColumnCoord(4, 1),
                    BlockColumnCoord(1, 2),
                    BlockColumnCoord(-2, -2),
                ],
            ),
        );
    }

    #[test]
    fn point_outside_polygon() {
        assert_eq!(
            InOutSide::Outside,
            point_position_relative_to_polygon(
                BlockColumnCoord(3, -4),
                &[
                    BlockColumnCoord(2, -1),
                    BlockColumnCoord(4, 1),
                    BlockColumnCoord(1, 2),
                    BlockColumnCoord(-2, -2),
                ],
            ),
        );
    }

    #[test]
    fn angle_0_deg() {
        assert_eq!(
            0f32,
            LandUsageGraph::angle(
                BlockColumnCoord(-2, -4),
                BlockColumnCoord(0, 0),
                BlockColumnCoord(2, 4),
            ),
        );
    }

    #[test]
    fn angle_pos_90_deg() {
        assert_eq!(
            PI / 2.0,
            LandUsageGraph::angle(
                BlockColumnCoord(-2, -4),
                BlockColumnCoord(0, 0),
                BlockColumnCoord(-4, 2),
            ),
        );
    }

    #[test]
    fn angle_neg_90_deg() {
        assert_eq!(
            -PI / 2.0,
            LandUsageGraph::angle(
                BlockColumnCoord(-2, -4),
                BlockColumnCoord(0, 0),
                BlockColumnCoord(4, -2),
            ),
        );
    }

    #[test]
    fn angle_180_deg() {
        assert_eq!(
            -PI,
            LandUsageGraph::angle(
                BlockColumnCoord(-2, -4),
                BlockColumnCoord(0, 0),
                BlockColumnCoord(-2, -4),
            ),
        );
    }
}
