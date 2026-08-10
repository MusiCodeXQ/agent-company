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

    OfficeLayout {
        width: 1600,
        height: 900,
        rooms,
        furniture: vec![
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
        ],
        desks,
        seats,
        doors: vec![
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
        ],
        spawn_points: vec![SpawnPoint {
            id: "visitor-entry".to_string(),
            x: 85,
            y: 640,
        }],
        navigation_obstacles: vec![
            Rect {
                x: 1190,
                y: 205,
                width: 270,
                height: 76,
            },
            Rect {
                x: 1180,
                y: 455,
                width: 230,
                height: 64,
            },
        ],
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
