mod registry;

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
use std::any::Any;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use serde_json::Value;

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
    app.add_systems(Update, show_egui);
    app.register_function("add_i32", add_i32);
    app.register_type::<Transform>();
    app.run();
}

fn show_egui(
    mut contexts: EguiContexts,
    mut snarl: Local<Snarl<Node>>,
    function_registry: NonSend<FunctionRegistry>,
    mut type_registry: ResMut<AppTypeRegistry>,
    component_map: Res<ComponentMap>,
) {
    let mut viewer = Viewer {
        function_registry: Some(&function_registry),
        type_registry: Some(type_registry),
        component_map: Some(component_map),
    };
    let style = SnarlStyle::default();
    bevy_egui::egui::CentralPanel::default().show(contexts.ctx_mut(), |ui| {
        ui.label("world");
        snarl
            .deref_mut()
            .show(&mut viewer, &style, bevy_egui::egui::Id::new("snarl"), ui);
    });
}

pub fn registry() -> Vec<Node> {
    let mut nodes = vec![];

    nodes.push(Node::Function(FunctionNode {
        function_info: add_i32.into_function().info().clone().with_name("add_i32"),
    }));

    nodes
}

fn add_i32(a: i32, b: i32) -> i32 {
    a + b
}

pub enum Node {
    Function(FunctionNode),
    Type(TypeNode),
    Query(QueryNode),
}

pub struct TypeNode {
    pub output_breakdown: bool,
    pub input_breakdown: bool,
    pub value: Box<dyn Reflect>,
}

impl TypeNode {
    pub fn new(value: Box<dyn Reflect>) -> Self {
        Self {
            output_breakdown: false,
            input_breakdown: true,
            value,
        }
    }
}

pub struct QueryNode {
    pub components: Vec<(String, ComponentId, TypeInfo)>,
}

pub struct FunctionNode {
    function_info: FunctionInfo,
}

#[derive(Serialize, Deserialize)]
struct Viewer<'a> {
    #[serde(skip)]
    function_registry: Option<&'a FunctionRegistry>,
    #[serde(skip)]
    type_registry: Option<ResMut<'a, AppTypeRegistry>>,
    #[serde(skip)]
    component_map: Option<Res<'a, ComponentMap>>,
}

impl SnarlViewer<Node> for Viewer<'_> {
    fn title(&mut self, node: &Node) -> String {
        match node {
            Node::Function(function) => function
                .function_info
                .name()
                .unwrap_or("unknown_name")
                .to_string(),
            Node::Type(reflect) => reflect
                .value
                .reflect_type_ident()
                .unwrap_or("unknown_type")
                .to_string(),
            Node::Query(_) => "query".to_string(),
        }
    }

    #[inline]
    fn has_body(&mut self, node: &Node) -> bool {
        match node {
            Node::Query(_) | Node::Type(_) => true,
            _ => false,
        }
    }

    /// Renders the node's body.
    #[inline]
    fn show_body(
        &mut self,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        ui: &mut Ui,
        scale: f32,
        snarl: &mut Snarl<Node>,
    ) {
        let node = &mut snarl[node];
        let is_query = match node {
            Node::Function(_) => unreachable!(),
            Node::Type(_) => false,
            Node::Query(_) => true,
        };
        if is_query {
            ui.button("right click to add component")
                .context_menu(|ui| {
                    for ty in self.type_registry.as_mut().unwrap().read().iter() {
                        if !self.component_map.as_ref().unwrap().0.contains_key(&ty.type_id()) {
                            continue;
                        }

                        match ty.type_info() {
                            TypeInfo::Struct(_) | TypeInfo::Value(_) => {}
                            _ => continue,
                        }
                        if ui.button(ty.type_info().type_path()).clicked() {
                            match node {
                                Node::Query(query) => query.components.push((
                                    ty.type_info().type_path().to_string(),
                                    self.component_map
                                        .as_ref()
                                        .unwrap()
                                        .0
                                        .get(&ty.type_id())
                                        .unwrap()
                                        .clone(),
                                    ty.type_info().clone(),
                                )),
                                _ => {}
                            }
                            ui.close_menu();
                        }
                    }
                });
        } else {
            match node {
                Node::Type(r#type) => {
                    let TypeNode { output_breakdown, input_breakdown, value } = r#type;
                    ui.checkbox(input_breakdown, "collapse input");
                    ui.checkbox(output_breakdown, "collapse output");
                }
                _ => unreachable!(),
            }
        }
    }

    fn outputs(&mut self, node: &Node) -> usize {
        match node {
            Node::Function(_) => {
                2 // output + direction
            }
            Node::Type(reflect) => match reflect.value.get_represented_type_info().unwrap() {
                TypeInfo::Struct(struct_info) => if reflect.output_breakdown { 1 } else { struct_info.field_len() },
                _ => 1,
            },
            Node::Query(query) => query.components.len(),
        }
    }

    fn inputs(&mut self, node: &Node) -> usize {
        match node {
            Node::Function(function) => {
                function.function_info.arg_count() + 1 // + direction
            }
            Node::Type(reflect) => match reflect.value.get_represented_type_info().unwrap() {
                TypeInfo::Struct(struct_info) => if reflect.input_breakdown { 1 } else { struct_info.field_len() },
                _ => 1,
            },
            Node::Query(_) => 0,
        }
    }
    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut Ui,
        scale: f32,
        snarl: &mut Snarl<Node>,
    ) -> PinInfo {
        let node = &mut snarl[pin.id.node];

        let mut t = match &mut self.type_registry {
            None => {
                panic!()
            }
            Some(a) => a.clone(),
        };
        match node {
            Node::Function(function) => {
                let color_type = if pin.remotes.len() == 0 {
                    UNTYPED_COLOR
                } else {
                    NUMBER_COLOR
                };
                if function.function_info.arg_count() <= pin.id.input {
                    PinInfo::square()
                } else {
                    PinInfo::circle()
                }
                .with_fill(color_type)
            }
            Node::Type(ref mut reflect) => {
                ui.push_id(pin.id, |ui| {
                    if reflect.input_breakdown {
                        return;
                    }
                    let reflect = reflect.value.as_reflect_mut();
                    let a = t.read();
                    match reflect.reflect_mut() {
                        ReflectMut::Struct(r#struct) => {
                            if pin.remotes.len() != 0 {
                                return;
                            }
                            let value = r#struct.field_at_mut(pin.id.input).unwrap();
                            ui.set_max_size(emath::Vec2::new(100.0, 100.0));
                            bevy_inspector_egui::reflect_inspector::ui_for_value(
                                value,
                                ui,
                                a.deref(),
                            );
                        }
                        ReflectMut::Value(value) => {
                            bevy_inspector_egui::reflect_inspector::ui_for_value(
                                value,
                                ui,
                                a.deref(),
                            );
                        }
                        _ => {
                            ui.label("todo");
                        }
                    }
                });
                PinInfo::circle()
            }
            Node::Query(_) => {
                PinInfo::circle() // we aren't actually ever gonna draw this cause it has no inputs
            }
        }
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut Ui,
        scale: f32,
        snarl: &mut Snarl<Node>,
    ) -> PinInfo {
        let node = &mut snarl[pin.id.node];
        let color_type = if pin.remotes.len() == 0 {
            UNTYPED_COLOR
        } else {
            NUMBER_COLOR
        };
        match node {
            Node::Function(_) => match pin.id.output {
                0 => PinInfo::circle(),
                1 => PinInfo::square(),
                _ => todo!(),
            }
            Node::Type(ref mut reflect) => {
                if reflect.output_breakdown {
                    return PinInfo::circle();
                }
                let reflect = reflect.value.as_reflect_mut();
                match reflect.reflect_mut() {
                    ReflectMut::Struct(r#struct) => {
                        if pin.remotes.len() != 0 {
                            return PinInfo::circle();
                        }
                        let value = r#struct.field_at_mut(pin.id.output).unwrap();
                        ui.set_max_size(emath::Vec2::new(200.0, 200.0));
                        ui.add_space(15.0);
                        ui.label(format!("{}", value.get_represented_type_info().unwrap().type_path()));
                    }
                    ReflectMut::Value(value) => {
                        ui.set_max_size(emath::Vec2::new(200.0, 200.0));
                        ui.add_space(15.0);
                        ui.label(format!("{}", value.get_represented_type_info().unwrap().type_path()));
                    }
                    _ => {
                        ui.label("todo");
                    }
                }
                PinInfo::circle()
            }
            Node::Query(ref mut query) => {
                let (name, component_id, type_info) = query.components.get(pin.id.output).unwrap().clone();
                if ui.button("-").clicked() {
                    query.components.remove(pin.id.output);
                }
                ui.label(format!("{name}"));
                PinInfo::circle()
            }
        }.with_fill(color_type)
    }

    fn has_graph_menu(&mut self, _pos: egui::Pos2, _snarl: &mut Snarl<Node>) -> bool {
        true
    }

    fn show_graph_menu(&mut self, pos: Pos2, ui: &mut Ui, scale: f32, snarl: &mut Snarl<Node>) {
        ScrollArea::both().show(ui, |ui| {
            if ui.button("add query").clicked() {
                snarl.insert_node(pos, Node::Query(QueryNode { components: vec![] }));
                ui.close_menu();
            }

            for (s, f) in self.function_registry.unwrap().0.iter() {
                if ui.button(s).clicked() {
                    snarl.insert_node(
                        pos,
                        Node::Function(FunctionNode {
                            function_info: f.info().clone().with_name(s.clone()),
                        }),
                    );
                    ui.close_menu();
                }
            }
            for ty in self.type_registry.as_mut().unwrap().read().iter() {
                let Some(default) = ty.data::<ReflectDefault>() else {
                    continue;
                };
                match ty.type_info() {
                    TypeInfo::Struct(_) | TypeInfo::Value(_) => {}
                    _ => continue,
                }
                if ui.button(ty.type_info().type_path()).clicked() {
                    let n = Node::Type(TypeNode::new(default.default()));
                    snarl.insert_node(pos, n);
                    ui.close_menu();
                }
            }
        });
    }
}

pub enum Bytecode {
    Push(Box<dyn Reflect>),
    Pop,
    Call(String),
}

fn instructions() -> Vec<Bytecode> {
    vec![
        Bytecode::Push(Box::new(2_i32)),
        Bytecode::Push(Box::new(1_i32)),
        Bytecode::Call("add_i32".to_string()),
        Bytecode::Call("print_i32".to_string()),
    ]
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

fn run() {
    let mut instructions = instructions();
    let mut stack = Vec::new();
    let mut functions = functions();
    for instruction in instructions {
        match instruction {
            Bytecode::Push(val) => {
                stack.push(val);
            }
            Bytecode::Pop => {
                stack.pop();
            }
            Bytecode::Call(function) => {
                let (func, arg_number) = functions.get_mut(&function).unwrap();
                let mut args = ArgList::new();
                for _ in 0..*arg_number {
                    args = args.push_boxed(stack.pop().unwrap());
                }
                match func.call(args).unwrap() {
                    Return::Unit => {}
                    Return::Owned(val) => stack.push(val),
                    Return::Ref(_) => {}
                    Return::Mut(_) => {}
                }
            }
        }
    }
}
