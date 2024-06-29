mod registry;
mod scripting;
mod compiler;
mod virtual_machine;
mod indirect_stack;

use crate::registry::{ComponentMap, FunctionRegistry, RegisterFunction, RegistryPlugin};
use bevy::ecs::component::{ComponentId, Components};
use bevy::prelude::*;
use bevy::reflect::func::{ArgList, Function, FunctionInfo, IntoFunction, Return};
use bevy::reflect::{ReflectMut, TypeInfo, TypeRegistry, TypeRegistryArc};
use bevy::DefaultPlugins;
use bevy_egui::egui::{emath, Color32, Pos2, ScrollArea, Ui};
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use bevy_inspector_egui::bevy_inspector::short_circuit;
use bevy_inspector_egui::inspector_egui_impls::InspectorEguiImpl;
use bevy_inspector_egui::reflect_inspector::InspectorUi;
use bevy_inspector_egui::DefaultInspectorConfigPlugin;
use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer};
use egui_snarl::{InPin, NodeId, OutPin, Snarl};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use bevy::ecs::system::SystemId;
use crate::virtual_machine::run;
/*use crate::virtual_machine::run;*/

const STRING_COLOR: Color32 = Color32::from_rgb(0x00, 0xb0, 0x00);
const NUMBER_COLOR: Color32 = Color32::from_rgb(0xb0, 0x00, 0x00);
const IMAGE_COLOR: Color32 = Color32::from_rgb(0xb0, 0x00, 0xb0);
const UNTYPED_COLOR: Color32 = Color32::from_rgb(0xb0, 0xb0, 0xb0);

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins,
        DefaultInspectorConfigPlugin,
        EguiPlugin,
        RegistryPlugin,
    ));
    app.add_systems(Update, (show_egui, print_transforms));
    app.add_systems(Startup, add_transforms);
    app.register_function("add_i32", add_i32);
    app.register_function("add_f32", add_f32);
    app.insert_resource(SnarlResource::default());
    app.register_type::<Transform>();
    app.run();
}

static mut SYSTEM_ID: Option<SystemId> = None;

fn add_transforms(mut commands: Commands) {
    commands.spawn(Transform::from_translation(Vec3::new(3.0, 3.0, 3.0)));
    commands.spawn(Transform::from_translation(Vec3::new(5.0, 5.0, 5.0)));
    unsafe {
        SYSTEM_ID.replace(commands.register_one_shot_system(run_vm_system));
    }
}

fn print_transforms(transforms: Query<(Entity, &Transform), Changed<Transform>>) {
    for (entity, t) in transforms.iter() {
        println!("entity: {}, translation: {:?}", entity, t.translation);
    }
}

fn run_vm_system(world: &mut World) {
    world.resource_scope(|world, snarl: Mut<SnarlResource>| {
        let instructions = crate::compiler::compile(&snarl.0);
        run(instructions, world);
    });
}

#[derive(Resource, Default)]
struct SnarlResource(pub Snarl<scripting::ScriptNode>);

fn show_egui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut snarl: ResMut<SnarlResource>,
    function_registry: NonSend<FunctionRegistry>,
    mut type_registry: ResMut<AppTypeRegistry>,
    component_map: Res<ComponentMap>,
) {
    let mut viewer = crate::scripting::Viewer {
        function_registry: Some(&function_registry),
        type_registry: Some(type_registry),
        component_map: Some(component_map),
    };
    let style = SnarlStyle::default();
    bevy_egui::egui::CentralPanel::default().show(contexts.ctx_mut(), |ui| {
        ui.label("world");
        if ui.button("run").clicked() {
            commands.run_system(unsafe {
                SYSTEM_ID.unwrap()
            });
        }
        snarl.0
            .show(&mut viewer, &style, bevy_egui::egui::Id::new("snarl"), ui);
    });
}

fn add_i32(a: i32, b: i32) -> i32 {
    a + b
}

fn add_f32(a: f32, b: f32) -> f32 {
    a + b
}

fn functions() -> HashMap<String, (Function<'static>, u32)> {
    let mut functions = HashMap::new();

    functions.insert(
        "add_i32".to_string(),
        ((|a: i32, b: i32| a + b).into_function(), 2),
    );
    functions.insert(
        "print_i32".to_string(),
        ((|a: i32| println!("{a}")).into_function(), 1),
    );
    functions
}
