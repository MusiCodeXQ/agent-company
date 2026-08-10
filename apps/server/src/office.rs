use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeLayout {
    pub width: u32,
    pub height: u32,
    pub rooms: Vec<RoomDefinition>,
    pub furniture: Vec<FurnitureDefinition>,
    pub desks: Vec<DeskDefinition>,
    pub seats: Vec<SeatDefinition>,
    pub doors: Vec<DoorDefinition>,
    pub spawn_points: Vec<SpawnPoint>,
    pub navigation_obstacles: Vec<Rect>,
    pub locations: Vec<LocationDefinition>,
    pub movement: MovementConfig,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomDefinition {
    pub id: String,
    #[serde(rename = "type")]
    pub room_type: String,
    pub label: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FurnitureDefinition {
    pub id: String,
    pub kind: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeskDefinition {
    pub id: String,
    pub employee_id: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub seat_id: String,
    pub monitor_x: u32,
    pub monitor_y: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatDefinition {
    pub id: String,
    pub kind: String,
    pub x: u32,
    pub y: u32,
    pub room_id: Option<String>,
    pub employee_id: Option<String>,
    pub desk_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoorDefinition {
    pub id: String,
    pub room_id: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnPoint {
    pub id: String,
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationDefinition {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub location_type: String,
    pub room_id: Option<String>,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovementConfig {
    pub base_speed: f32,
    pub grid_cell_size: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub fn default_office() -> OfficeLayout {
    let rooms = vec![
        room("open-office", "workspace", "开放办公区", 260, 120, 850, 580),
        room("meeting-a", "meeting", "协作会议室", 1130, 120, 390, 250),
        room("interview-room", "interview", "面试室", 1130, 390, 390, 210),
        room("hr-office", "office", "HR 办公室", 40, 120, 190, 260),
        room("reception", "reception", "前台接待", 40, 410, 190, 180),
        room("pantry", "pantry", "茶水间", 1130, 620, 220, 200),
        room("restroom", "restroom", "卫生间", 1370, 620, 150, 200),
    ];

    let desk_specs = [
        ("desk-supervisor-01", "supervisor-01", 310, 180),
        ("desk-product-01", "product-01", 515, 180),
        ("desk-ui-01", "ui-01", 720, 180),
        ("desk-frontend-01", "frontend-01", 925, 180),
        ("desk-backend-01", "backend-01", 410, 450),
        ("desk-qa-01", "qa-01", 650, 450),
        ("desk-hr-01", "hr-01", 70, 210),
    ];
    let mut desks = Vec::new();
    let mut seats = Vec::new();
    for (id, employee_id, x, y) in desk_specs {
        let seat_id = format!("seat-{employee_id}");
        desks.push(DeskDefinition {
            id: id.to_string(),
            employee_id: employee_id.to_string(),
            x,
            y,
            width: 160,
            height: 82,
            seat_id: seat_id.clone(),
            monitor_x: x + 58,
            monitor_y: y + 14,
        });
        seats.push(SeatDefinition {
            id: seat_id,
            kind: "desk".to_string(),
            x: x + 80,
            y: y + 108,
            room_id: Some(if employee_id == "hr-01" {
                "hr-office".to_string()
            } else {
                "open-office".to_string()
            }),
            employee_id: Some(employee_id.to_string()),
            desk_id: Some(id.to_string()),
        });
    }

    seats.extend([
        SeatDefinition {
            id: "meeting-seat-backend".to_string(),
            kind: "meeting".to_string(),
            x: 1280,
            y: 245,
            room_id: Some("meeting-a".to_string()),
            employee_id: Some("backend-01".to_string()),
            desk_id: None,
        },
        SeatDefinition {
            id: "meeting-seat-2".to_string(),
            kind: "meeting".to_string(),
            x: 1350,
            y: 245,
            room_id: Some("meeting-a".to_string()),
            employee_id: None,
            desk_id: None,
        },
        SeatDefinition {
            id: "interview-seat-1".to_string(),
            kind: "interview".to_string(),
            x: 1230,
            y: 505,
            room_id: Some("interview-room".to_string()),
            employee_id: None,
            desk_id: None,
        },
    ]);

    let furniture = vec![
        furniture("meeting-table", "meeting-table", 1190, 205, 270, 76, None),
        furniture(
            "interview-table",
            "interview-table",
            1180,
            455,
            230,
            64,
            None,
        ),
        furniture("hr-cabinet", "cabinet", 65, 145, 130, 34, Some("人才档案")),
        furniture(
            "reception-counter",
            "reception-counter",
            58,
            500,
            150,
            38,
            Some("VISITOR"),
        ),
        furniture(
            "pantry-counter",
            "pantry-counter",
            1160,
            700,
            160,
            40,
            Some("COFFEE"),
        ),
        furniture("coffee-machine", "coffee-machine", 1175, 655, 46, 38, None),
        furniture(
            "water-dispenser",
            "water-dispenser",
            1260,
            655,
            38,
            38,
            None,
        ),
        furniture("restroom-sinks", "restroom-sinks", 1390, 660, 105, 38, None),
    ];
    let navigation_obstacles = desks
        .iter()
        .map(|desk| Rect {
            x: desk.x,
            y: desk.y,
            width: desk.width,
            height: desk.height + 14,
        })
        .chain(furniture.iter().map(|item| Rect {
            x: item.x,
            y: item.y,
            width: item.width,
            height: item.height,
        }))
        .collect();

    let doors = vec![
        DoorDefinition {
            id: "door-open-office".to_string(),
            room_id: "open-office".to_string(),
            x: 1100,
            y: 340,
            width: 30,
            height: 70,
        },
        DoorDefinition {
            id: "door-meeting".to_string(),
            room_id: "meeting-a".to_string(),
            x: 1120,
            y: 220,
            width: 25,
            height: 54,
        },
        DoorDefinition {
            id: "door-interview".to_string(),
            room_id: "interview-room".to_string(),
            x: 1120,
            y: 455,
            width: 25,
            height: 54,
        },
        DoorDefinition {
            id: "door-reception".to_string(),
            room_id: "reception".to_string(),
            x: 220,
            y: 480,
            width: 25,
            height: 54,
        },
        DoorDefinition {
            id: "door-pantry".to_string(),
            room_id: "pantry".to_string(),
            x: 1120,
            y: 680,
            width: 25,
            height: 54,
        },
        DoorDefinition {
            id: "door-restroom".to_string(),
            room_id: "restroom".to_string(),
            x: 1355,
            y: 680,
            width: 25,
            height: 54,
        },
    ];
    let spawn_points = spawn_points();
    let locations = default_locations(&desks, &seats, &doors, &spawn_points);

    OfficeLayout {
        width: 1600,
        height: 900,
        rooms,
        furniture,
        desks,
        seats,
        doors,
        spawn_points,
        navigation_obstacles,
        locations,
        movement: MovementConfig {
            base_speed: 120.0,
            grid_cell_size: 20,
        },
    }
}

fn spawn_points() -> Vec<SpawnPoint> {
    vec![SpawnPoint {
        id: "visitor-entry".to_string(),
        x: 85,
        y: 640,
    }]
}

fn default_locations(
    desks: &[DeskDefinition],
    seats: &[SeatDefinition],
    doors: &[DoorDefinition],
    spawn_points: &[SpawnPoint],
) -> Vec<LocationDefinition> {
    let mut locations = desks
        .iter()
        .filter_map(|desk| {
            let seat = seats.iter().find(|seat| seat.id == desk.seat_id)?;
            Some(LocationDefinition {
                id: workstation_location_id(&desk.employee_id),
                name: format!("{} 工位", desk.employee_id),
                location_type: "WORKSTATION".to_string(),
                room_id: seat.room_id.clone(),
                x: seat.x as f32,
                y: seat.y as f32,
            })
        })
        .collect::<Vec<_>>();

    locations.extend([
        location(
            "office_entrance",
            "公司入口",
            "ENTRANCE",
            Some("reception"),
            85.0,
            640.0,
        ),
        location(
            "reception_waiting_01",
            "前台等候位",
            "WAITING_POINT",
            Some("reception"),
            130.0,
            460.0,
        ),
        location(
            "meeting_room_door",
            "会议室门",
            "DOOR",
            Some("meeting-a"),
            1132.0,
            247.0,
        ),
        location(
            "meeting_seat_01",
            "会议座位 01",
            "MEETING_SEAT",
            Some("meeting-a"),
            1190.0,
            170.0,
        ),
        location(
            "meeting_seat_02",
            "会议座位 02",
            "MEETING_SEAT",
            Some("meeting-a"),
            1280.0,
            170.0,
        ),
        location(
            "meeting_seat_03",
            "会议座位 03",
            "MEETING_SEAT",
            Some("meeting-a"),
            1370.0,
            170.0,
        ),
        location(
            "meeting_seat_04",
            "会议座位 04",
            "MEETING_SEAT",
            Some("meeting-a"),
            1190.0,
            320.0,
        ),
        location(
            "meeting_seat_05",
            "会议座位 05",
            "MEETING_SEAT",
            Some("meeting-a"),
            1280.0,
            320.0,
        ),
        location(
            "meeting_seat_06",
            "会议座位 06",
            "MEETING_SEAT",
            Some("meeting-a"),
            1370.0,
            320.0,
        ),
        location(
            "meeting_seat_07",
            "会议座位 07",
            "MEETING_SEAT",
            Some("meeting-a"),
            1490.0,
            320.0,
        ),
        location(
            "interview_room_door",
            "面试室门",
            "DOOR",
            Some("interview-room"),
            1132.0,
            482.0,
        ),
        location(
            "interview_seat_hr",
            "面试 HR 座位",
            "INTERVIEW_SEAT",
            Some("interview-room"),
            1160.0,
            430.0,
        ),
        location(
            "interview_seat_candidate",
            "候选人座位",
            "INTERVIEW_SEAT",
            Some("interview-room"),
            1160.0,
            550.0,
        ),
        location(
            "interview_seat_technical_01",
            "技术面试座位 01",
            "INTERVIEW_SEAT",
            Some("interview-room"),
            1430.0,
            430.0,
        ),
        location(
            "interview_seat_technical_02",
            "技术面试座位 02",
            "INTERVIEW_SEAT",
            Some("interview-room"),
            1430.0,
            550.0,
        ),
        location(
            "coffee_point_01",
            "咖啡点",
            "COFFEE_POINT",
            Some("pantry"),
            1145.0,
            675.0,
        ),
        location(
            "toilet_entrance",
            "卫生间入口",
            "DOOR",
            Some("restroom"),
            1382.0,
            707.0,
        ),
        location(
            "open_office_path_01",
            "开放区通道",
            "PATH_POINT",
            Some("open-office"),
            1080.0,
            350.0,
        ),
    ]);

    for door in doors {
        let id = match door.room_id.as_str() {
            "meeting-a" => "meeting_room_door",
            "interview-room" => "interview_room_door",
            _ => continue,
        };
        if let Some(existing) = locations.iter_mut().find(|item| item.id == id) {
            existing.x = door.x as f32 + door.width as f32 / 2.0;
            existing.y = door.y as f32 + door.height as f32 / 2.0;
        }
    }
    if let Some(entry) = spawn_points.first() {
        if let Some(existing) = locations
            .iter_mut()
            .find(|item| item.id == "office_entrance")
        {
            existing.x = entry.x as f32;
            existing.y = entry.y as f32;
        }
    }
    locations
}

pub fn workstation_location_id(employee_id: &str) -> String {
    match employee_id {
        "backend-01" => "backend_desk_01",
        "frontend-01" => "frontend_desk_01",
        "qa-01" => "qa_desk_01",
        "product-01" => "product_desk_01",
        "ui-01" => "ui_desk_01",
        "supervisor-01" => "supervisor_desk_01",
        "hr-01" => "hr_desk_01",
        other => return format!("{other}_desk_01"),
    }
    .to_string()
}

fn location(
    id: &str,
    name: &str,
    location_type: &str,
    room_id: Option<&str>,
    x: f32,
    y: f32,
) -> LocationDefinition {
    LocationDefinition {
        id: id.to_string(),
        name: name.to_string(),
        location_type: location_type.to_string(),
        room_id: room_id.map(str::to_string),
        x,
        y,
    }
}

fn room(
    id: &str,
    room_type: &str,
    label: &str,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> RoomDefinition {
    RoomDefinition {
        id: id.to_string(),
        room_type: room_type.to_string(),
        label: label.to_string(),
        x,
        y,
        width,
        height,
    }
}

fn furniture(
    id: &str,
    kind: &str,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    label: Option<&str>,
) -> FurnitureDefinition {
    FurnitureDefinition {
        id: id.to_string(),
        kind: kind.to_string(),
        x,
        y,
        width,
        height,
        label: label.map(str::to_string),
    }
}
