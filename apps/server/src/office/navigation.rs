use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
};

use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use serde_json::json;

use crate::{
    app_state::AppState,
    application::{a2a, organization},
    domain::{
        models::{AgentMovementSummary, Employee, MovementPoint},
        office::{self, LocationDefinition, OfficeLayout, Rect},
    },
    infrastructure::{db, events},
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct GridPoint {
    x: i32,
    y: i32,
}

pub async fn request_move(
    state: &AppState,
    company_id: &str,
    agent_id: &str,
    destination_location_id: &str,
    reason: &str,
) -> Result<AgentMovementSummary> {
    let employee = db::employees(&state.pool, company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == agent_id)
        .ok_or_else(|| anyhow!("agent {agent_id} was not found"))?;
    if let Some(active) =
        db::pending_or_active_movement_for_agent(&state.pool, company_id, agent_id).await?
    {
        return Err(anyhow!("agent already has active movement {}", active.id));
    }
    let floor_id = employee.floor_id.as_deref().unwrap_or("floor-01");
    let floor = organization::floor(&state.pool, company_id, floor_id).await?;
    let workstation_inputs =
        organization::workstation_layout_inputs(&state.pool, company_id, floor_id).await?;
    let layout = office::office_for_floor(&floor.id, &floor.name, &workstation_inputs);
    let destination = layout
        .locations
        .iter()
        .find(|location| location.id == destination_location_id)
        .ok_or_else(|| anyhow!("destination location does not exist: {destination_location_id}"))?;
    let from_location_id = employee
        .location_id
        .clone()
        .unwrap_or_else(|| office::workstation_location_id(agent_id));
    let from = MovementPoint {
        x: employee.x,
        y: employee.y,
    };
    let from_room_id = employee.room_id.as_deref().or_else(|| {
        layout
            .locations
            .iter()
            .find(|location| location.id == from_location_id)
            .and_then(|location| location.room_id.as_deref())
    });
    let path = build_route(&layout, from, from_room_id, destination)?;
    let distance = path
        .windows(2)
        .map(|points| {
            let dx = points[1].x - points[0].x;
            let dy = points[1].y - points[0].y;
            f64::from((dx * dx + dy * dy).sqrt())
        })
        .sum::<f64>();
    let seconds = (distance / f64::from(layout.movement.base_speed)).max(0.5);
    let started_at = Utc::now();
    let estimated_arrival_at = started_at + Duration::milliseconds((seconds * 1000.0) as i64);
    let reserved_until = started_at + Duration::hours(2);
    let movement = db::create_movement(
        &state.pool,
        company_id,
        agent_id,
        &from_location_id,
        destination_location_id,
        &path,
        &started_at.to_rfc3339(),
        &estimated_arrival_at.to_rfc3339(),
        &reserved_until.to_rfc3339(),
        reason,
    )
    .await?;
    if reason != "A2A_FACE_TO_FACE" {
        a2a::close_sessions_for_departing_agent(
            state,
            company_id,
            agent_id,
            destination_location_id,
        )
        .await?;
    }
    events::emit(
        state,
        company_id,
        "AGENT_MOVE_REQUESTED",
        Some(agent_id),
        Some(&movement.id),
        json!({ "movement": movement, "reason": reason }),
    )
    .await?;
    state.worker_notify.notify_one();
    Ok(movement)
}

pub fn build_route(
    layout: &OfficeLayout,
    from: MovementPoint,
    from_room_id: Option<&str>,
    destination: &LocationDefinition,
) -> Result<Vec<MovementPoint>> {
    let mut waypoints = Vec::new();
    if let Some(door) = transition_door(layout, from_room_id, destination.room_id.as_deref()) {
        waypoints.push(MovementPoint {
            x: door.x,
            y: door.y,
        });
    }
    waypoints.push(MovementPoint {
        x: destination.x,
        y: destination.y,
    });

    let mut route = vec![from.clone()];
    let mut start = from;
    for waypoint in waypoints {
        let segment = astar_segment(layout, start.clone(), waypoint.clone())?;
        append_without_duplicate(&mut route, segment);
        start = waypoint;
    }
    Ok(route)
}

fn transition_door<'a>(
    layout: &'a OfficeLayout,
    from_room_id: Option<&str>,
    to_room_id: Option<&str>,
) -> Option<&'a LocationDefinition> {
    let room_id = match (from_room_id, to_room_id) {
        (Some("meeting-a"), _) | (_, Some("meeting-a")) => "meeting_room_door",
        (Some("interview-room"), _) | (_, Some("interview-room")) => "interview_room_door",
        _ => return None,
    };
    layout.locations.iter().find(|location| {
        location.id == room_id
            || location.id.ends_with(&format!("__{room_id}"))
            || location.room_id.as_deref() == Some(room_id)
    })
}

fn astar_segment(
    layout: &OfficeLayout,
    start: MovementPoint,
    target: MovementPoint,
) -> Result<Vec<MovementPoint>> {
    let cell_size = layout.movement.grid_cell_size as f32;
    let width = (layout.width as f32 / cell_size).ceil() as i32;
    let height = (layout.height as f32 / cell_size).ceil() as i32;
    let start_grid = to_grid(start.clone(), cell_size, width, height);
    let target_grid = to_grid(target.clone(), cell_size, width, height);
    if start_grid == target_grid {
        return Ok(vec![start, target]);
    }

    let mut open = BinaryHeap::new();
    let mut came_from = HashMap::<GridPoint, GridPoint>::new();
    let mut scores = HashMap::<GridPoint, i32>::new();
    let mut visited = HashSet::new();
    scores.insert(start_grid, 0);
    open.push((Reverse(heuristic(start_grid, target_grid)), start_grid));

    while let Some((Reverse(_priority), current)) = open.pop() {
        if !visited.insert(current) {
            continue;
        }
        if current == target_grid {
            let mut cells = vec![current];
            let mut cursor = current;
            while let Some(previous) = came_from.get(&cursor) {
                cells.push(*previous);
                cursor = *previous;
            }
            cells.reverse();
            let mut points = cells
                .into_iter()
                .map(|cell| grid_center(cell, cell_size))
                .collect::<Vec<_>>();
            if let Some(first) = points.first_mut() {
                *first = start;
            }
            if let Some(last) = points.last_mut() {
                *last = target;
            }
            return Ok(points);
        }
        let current_score = *scores.get(&current).unwrap_or(&i32::MAX);
        for neighbor in neighbors(current, width, height) {
            if neighbor != target_grid
                && is_blocked(neighbor, cell_size, &layout.navigation_obstacles)
            {
                continue;
            }
            let tentative = current_score + 1;
            if tentative < *scores.get(&neighbor).unwrap_or(&i32::MAX) {
                came_from.insert(neighbor, current);
                scores.insert(neighbor, tentative);
                open.push((
                    Reverse(tentative + heuristic(neighbor, target_grid)),
                    neighbor,
                ));
            }
        }
    }
    Err(anyhow!(
        "no walkable route from ({:.0},{:.0}) to ({:.0},{:.0})",
        start.x,
        start.y,
        target.x,
        target.y
    ))
}

fn to_grid(point: MovementPoint, cell_size: f32, width: i32, height: i32) -> GridPoint {
    GridPoint {
        x: ((point.x / cell_size).round() as i32).clamp(0, width - 1),
        y: ((point.y / cell_size).round() as i32).clamp(0, height - 1),
    }
}

fn grid_center(point: GridPoint, cell_size: f32) -> MovementPoint {
    MovementPoint {
        x: point.x as f32 * cell_size,
        y: point.y as f32 * cell_size,
    }
}

fn neighbors(point: GridPoint, width: i32, height: i32) -> impl Iterator<Item = GridPoint> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter_map(move |(dx, dy)| {
            let x = point.x + dx;
            let y = point.y + dy;
            (x >= 0 && y >= 0 && x < width && y < height).then_some(GridPoint { x, y })
        })
}

fn heuristic(left: GridPoint, right: GridPoint) -> i32 {
    (left.x - right.x).abs() + (left.y - right.y).abs()
}

fn is_blocked(point: GridPoint, cell_size: f32, obstacles: &[Rect]) -> bool {
    let x = point.x as f32 * cell_size;
    let y = point.y as f32 * cell_size;
    obstacles.iter().any(|obstacle| {
        x >= obstacle.x as f32 - 12.0
            && x <= (obstacle.x + obstacle.width) as f32 + 12.0
            && y >= obstacle.y as f32 - 12.0
            && y <= (obstacle.y + obstacle.height) as f32 + 12.0
    })
}

fn append_without_duplicate(route: &mut Vec<MovementPoint>, segment: Vec<MovementPoint>) {
    for point in segment {
        let duplicate = route.last().is_some_and(|last| {
            (last.x - point.x).abs() < f32::EPSILON && (last.y - point.y).abs() < f32::EPSILON
        });
        if !duplicate {
            route.push(point);
        }
    }
}

pub async fn employee_workstation_location(
    state: &AppState,
    company_id: &str,
    employee: &Employee,
) -> Result<String> {
    if let Some(desk_id) = employee.desk_id.as_deref() {
        if let Some(location_id) = sqlx::query_scalar::<_, String>(
            "SELECT location_id FROM workstations WHERE company_id = ? AND id = ?",
        )
        .bind(company_id)
        .bind(desk_id)
        .fetch_optional(&state.pool)
        .await?
        {
            return Ok(location_id);
        }
    }
    Ok(office::workstation_location_id(&employee.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_route_passes_through_the_meeting_door() {
        let layout = office::default_office();
        let destination = layout
            .locations
            .iter()
            .find(|location| location.id == "meeting_seat_02")
            .expect("meeting seat exists");
        let route = build_route(
            &layout,
            MovementPoint { x: 490.0, y: 558.0 },
            Some("open-office"),
            destination,
        )
        .expect("route should exist");
        assert!(route
            .iter()
            .any(|point| { (point.x - 1132.5).abs() < 1.0 && (point.y - 247.0).abs() < 1.0 }));
        assert_eq!(route.last().map(|point| point.x), Some(1280.0));
        assert_eq!(route.last().map(|point| point.y), Some(170.0));
    }

    #[test]
    fn route_does_not_use_the_meeting_table_as_a_walkable_cell() {
        let layout = office::default_office();
        let destination = layout
            .locations
            .iter()
            .find(|location| location.id == "meeting_seat_05")
            .expect("meeting seat exists");
        let route = build_route(
            &layout,
            MovementPoint { x: 730.0, y: 558.0 },
            Some("open-office"),
            destination,
        )
        .expect("route should exist");
        assert!(route.iter().all(|point| {
            !(point.x >= 1190.0 && point.x <= 1460.0 && point.y >= 205.0 && point.y <= 281.0)
        }));
    }
}
