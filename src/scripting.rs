use crate::registry::{ComponentMap, FunctionRegistry};
use crate::{NUMBER_COLOR, UNTYPED_COLOR};
use bevy::ecs::component::ComponentId;
use bevy::prelude::{AppTypeRegistry, ReflectDefault, Res, ResMut};
use bevy::reflect::func::FunctionInfo;
use bevy::reflect::{Reflect, ReflectMut, ReflectRef, TypeInfo};
use bevy_egui::egui::{emath, menu, ComboBox, Pos2, ScrollArea, Ui};
use egui_snarl::ui::{PinInfo, SnarlViewer};
use egui_snarl::{InPin, NodeId, OutPin, Snarl};
use serde::{Deserialize, Serialize};
use std::any::Any;

#[derive(Clone, Debug)]
pub enum ScriptNode {
    Set(SetNode),
    Field(FieldNode),
    Function(FunctionNode),
    TypeCreation(TypeCreationNode),
    Query(QueryNode),
}

impl ScriptNode {
    pub(crate) fn can_flow(&self) -> bool {
        match self {
            ScriptNode::Set(_) => true,
            ScriptNode::Field(_) => false,
            ScriptNode::Function(_) => true,
            ScriptNode::TypeCreation(_) => false,
            ScriptNode::Query(_) => true,
        }
    }
    fn set() -> Self {
        Self::Set(SetNode::new())
    }
    fn function(function_info: FunctionInfo) -> Self {
        Self::Function(FunctionNode::new(function_info))
    }
    fn field() -> Self {
        Self::Field(FieldNode::new())
    }
    fn type_creation(value: Box<dyn Reflect>) -> Self {
        Self::TypeCreation(TypeCreationNode::new(value))
    }
    fn query() -> Self {
        Self::Query(QueryNode::new())
    }
}
#[derive(Clone, Debug)]
pub struct SetNode {}
impl SetNode {
    pub fn new() -> Self {
        SetNode {}
    }
}
#[derive(Clone, Debug)]
pub struct FieldNode {
    pub field: Option<TypeInfo>,
    pub name: Option<String>,
}

impl FieldNode {
    pub fn convert_string(&self) -> String {
        if let Some(str) = &self.name {
            format!(
                "{}: {}",
                remove_before_double_colon(self.field.as_ref().unwrap().type_path()),
                str
            )
            .to_string()
        } else {
            remove_before_double_colon(self.field.as_ref().unwrap().type_path())
        }
    }
}

#[derive(Clone, Debug)]
pub struct TypeInfoWrapper(TypeInfo, Option<String>);

impl TypeInfoWrapper {
    pub fn convert_string(&self) -> String {
        if let Some(str) = &self.1 {
            format!(
                "{}: {}",
                remove_before_double_colon(self.0.type_path()),
                str
            )
            .to_string()
        } else {
            remove_before_double_colon(self.0.type_path())
        }
    }
}

impl PartialEq for TypeInfoWrapper {
    fn eq(&self, other: &Self) -> bool {
        let a = self.0.type_id() == other.type_id();
        if !a {
            return false;
        };
        match (&self.1, &other.1) {
            (Some(s1), Some(s2)) => s1 == s2,
            (None, None) => true,
            _ => false,
        }
    }
}

impl FieldNode {
    pub fn new() -> Self {
        FieldNode {
            field: None,
            name: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct FunctionNode {
    pub function_info: FunctionInfo,
}

impl FunctionNode {
    pub fn new(function_info: FunctionInfo) -> Self {
        FunctionNode { function_info }
    }
}
#[derive(Debug)]
pub struct TypeCreationNode {
    pub value: Box<dyn Reflect>,
}

impl Clone for TypeCreationNode {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone_value(),
        }
    }
}

impl TypeCreationNode {
    pub fn new(value: Box<dyn Reflect>) -> Self {
        TypeCreationNode { value }
    }
}

#[derive(Clone, Debug)]
pub struct QueryNode {
    pub components: Vec<(String, ComponentId, TypeInfo)>,
}

impl QueryNode {
    pub fn new() -> Self {
        QueryNode { components: vec![] }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Viewer<'a> {
    #[serde(skip)]
    pub(crate) function_registry: Option<&'a FunctionRegistry>,
    #[serde(skip)]
    pub(crate) type_registry: Option<ResMut<'a, AppTypeRegistry>>,
    #[serde(skip)]
    pub(crate) component_map: Option<Res<'a, ComponentMap>>,
}

fn remove_before_double_colon(s: &str) -> String {
    s.rsplit("::").next().unwrap_or(s).to_string()
}

impl SnarlViewer<ScriptNode> for Viewer<'_> {
    fn title(&mut self, node: &ScriptNode) -> String {
        match node {
            ScriptNode::Set(_set_node) => "set".to_string(),       //TODO
            ScriptNode::Field(_field_node) => "field".to_string(), //TODO
            ScriptNode::Function(function_node) => function_node
                .function_info
                .name()
                .unwrap_or("unknown_function")
                .to_string(),
            ScriptNode::TypeCreation(type_node) => type_node
                .value
                .reflect_type_ident()
                .unwrap_or("unknown_type")
                .to_string(),
            ScriptNode::Query(_query_node) => "query".to_string(), //TODO
        }
    }

    fn outputs(&mut self, node: &ScriptNode) -> usize {
        match node {
            ScriptNode::Set(_) => 1,                                          // the flow node
            ScriptNode::Field(_) => 1,                                        // just the data
            ScriptNode::Function(_) => 2,                                     // data + flow
            ScriptNode::TypeCreation(_) => 1,                                 // just the data
            ScriptNode::Query(query_node) => 1 + query_node.components.len(), //plus flow
        }
    }

    fn inputs(&mut self, node: &ScriptNode) -> usize {
        match node {
            ScriptNode::Set(_) => 3,   // the flow, the data, and the replacement,
            ScriptNode::Field(_) => 1, // just the input struct
            ScriptNode::Function(function_node) => function_node.function_info.arg_count() + 1, // plus flow node
            ScriptNode::TypeCreation(type_creation_node) => {
                match type_creation_node.value.reflect_ref() {
                    ReflectRef::Struct(dyn_struct) => {
                        dyn_struct.field_len() // with no flow node
                    }
                    ReflectRef::Value(_) => 1,
                    _ => todo!(),
                }
            }
            ScriptNode::Query(_) => 0,
        }
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut Ui,
        scale: f32,
        snarl: &mut Snarl<ScriptNode>,
    ) -> PinInfo {
        let node = &mut snarl[pin.id.node];
        let color = if pin.remotes.len() == 0 {
            UNTYPED_COLOR
        } else {
            NUMBER_COLOR
        };
        let type_registry = match &mut self.type_registry {
            None => {
                panic!()
            }
            Some(a) => a.clone(),
        };
        let type_registry = type_registry.read();
        match node {
            ScriptNode::Set(_) => if pin.id.input == 0 {
                PinInfo::triangle()
            } else {
                PinInfo::circle()
            }
            .with_fill(color),
            ScriptNode::Field(field_node) => {
                let Some(first) = pin.remotes.first() else {
                    field_node.field = None;
                    return PinInfo::circle().with_fill(color);
                };
                drop(node);
                let output = &mut snarl[first.node];
                let mut fields = vec![];
                match output {
                    ScriptNode::Field(field) => match &field.field {
                        None => {}
                        Some(type_info) => match type_info.clone() {
                            TypeInfo::Struct(struct_info) => {
                                fields = struct_info
                                    .iter()
                                    .map(|a| {
                                        TypeInfoWrapper(
                                            self.type_registry
                                                .as_ref()
                                                .unwrap()
                                                .read()
                                                .get(a.type_id())
                                                .unwrap()
                                                .type_info()
                                                .clone(),
                                            Some(a.name().to_string()),
                                        )
                                    })
                                    .collect::<Vec<_>>();
                            }
                            TypeInfo::Value(_) => {
                                todo!()
                            }
                            _ => {
                                todo!()
                            }
                        },
                    },
                    ScriptNode::Function(_) => todo!(),
                    ScriptNode::TypeCreation(type_creation) => match type_creation.value.reflect_ref() {
                        ReflectRef::Struct(dyn_struct) => {
                            for (index, f) in dyn_struct.iter_fields().enumerate() {
                                fields.push(TypeInfoWrapper(
                                    f.get_represented_type_info().unwrap().clone(),
                                    Some(
                                        dyn_struct.name_at(index).unwrap().clone().parse().unwrap(),
                                    ),
                                ));
                            }
                        }
                        ReflectRef::Value(_) => {
                            todo!()
                        }
                        _ => todo!(),
                    },
                    ScriptNode::Query(query_node) => {
                        let (name, id, type_info) =
                            query_node.components.get(first.output - 1).unwrap();
                        match type_info.clone() {
                            TypeInfo::Struct(struct_info) => {
                                fields = struct_info
                                    .iter()
                                    .map(|a| {
                                        TypeInfoWrapper(
                                            self.type_registry
                                                .as_ref()
                                                .unwrap()
                                                .read()
                                                .get(a.type_id())
                                                .unwrap()
                                                .type_info()
                                                .clone(),
                                            Some(a.name().to_string()),
                                        )
                                    })
                                    .collect::<Vec<_>>();
                            }
                            TypeInfo::Value(_) => {
                                todo!()
                            }
                            _ => {
                                todo!()
                            }
                        }
                    }
                    _ => {
                        panic!("shouldn't reach here")
                    }
                }
                drop(output);
                let f = fields.clone();
                let node = &mut snarl[pin.id.node];
                let ScriptNode::Field(this_field) = node else {
                    unreachable!()
                };
                if this_field.field.is_none() {
                    this_field
                        .field
                        .replace(f.first().as_ref().unwrap().0.clone());
                    this_field.name = f.first().as_ref().unwrap().1.clone();
                }
                ComboBox::from_label("field")
                    .selected_text(this_field.convert_string())
                    .show_ui(ui, |ui| {
                        let mut temp_this_field = TypeInfoWrapper(
                            this_field.field.as_ref().unwrap().clone(),
                            this_field.name.clone(),
                        );
                        for field in f {
                            let name = field.convert_string();
                            ui.selectable_value(&mut temp_this_field, field, name);
                        }
                        this_field.field.replace(temp_this_field.0);
                        this_field.name = temp_this_field.1;
                    });
                PinInfo::circle().with_fill(color)
            }
            ScriptNode::Function(_) => if pin.id.input == 0 {
                PinInfo::triangle()
            } else {
                PinInfo::circle()
            }
            .with_fill(color),
            ScriptNode::TypeCreation(type_creation_node) => {
                match type_creation_node.value.reflect_mut() {
                    ReflectMut::Struct(dyn_struct) => {
                        let value = dyn_struct.field_at_mut(pin.id.input).unwrap();
                        ui.set_max_size(emath::Vec2::new(150.0, 150.0));
                        ui.add_space(20.0);
                        ui.label(value.reflect_type_ident().unwrap_or("unknown"));
                        bevy_inspector_egui::reflect_inspector::ui_for_value(
                            value,
                            ui,
                            &type_registry,
                        );
                    }
                    ReflectMut::Value(value) => {
                        bevy_inspector_egui::reflect_inspector::ui_for_value(
                            value,
                            ui,
                            &type_registry,
                        );
                    }
                    _ => {}
                }
                PinInfo::circle().with_fill(color)
            }
            ScriptNode::Query(_) => unreachable!(), //no inputs for queries
        }
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut Ui,
        scale: f32,
        snarl: &mut Snarl<ScriptNode>,
    ) -> PinInfo {
        let node = &mut snarl[pin.id.node];
        let color = if pin.remotes.len() == 0 {
            UNTYPED_COLOR
        } else {
            NUMBER_COLOR
        };
        match node {
            ScriptNode::Set(_) => {
                PinInfo::triangle().with_fill(color) // only flow node output
            }
            ScriptNode::Field(_) => PinInfo::circle().with_fill(color),
            ScriptNode::Function(_) => if pin.id.output == 0 {
                PinInfo::triangle()
            } else {
                PinInfo::circle()
            }
            .with_fill(color),
            ScriptNode::TypeCreation(_) => PinInfo::circle().with_fill(color), // no flow nodes just a single data
            ScriptNode::Query(query_node) => if pin.id.output == 0 {
                PinInfo::triangle()
            } else {
                ui.label(&query_node.components.get(pin.id.output - 1).unwrap().0);
                PinInfo::circle()
            }
            .with_fill(color),
        }
    }

    #[inline]
    fn has_body(&mut self, node: &ScriptNode) -> bool {
        match node {
            ScriptNode::Query(_) | ScriptNode::TypeCreation(_) => true,
            _ => false,
        }
    }

    #[inline]
    fn show_body(
        &mut self,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        ui: &mut Ui,
        scale: f32,
        snarl: &mut Snarl<ScriptNode>,
    ) {
        let node = &mut snarl[node];
        match node {
            ScriptNode::Set(_) => {}
            ScriptNode::Field(_) => {}
            ScriptNode::Function(_) => {}
            ScriptNode::TypeCreation(_) => {}
            ScriptNode::Query(query) => {
                ui.menu_button("Add Component", |ui| {
                    for ty in self.type_registry.as_mut().unwrap().read().iter() {
                        let name = remove_before_double_colon(ty.type_info().type_path());
                        match ty.type_info() {
                            TypeInfo::Struct(_) | TypeInfo::Value(_) => {}
                            _ => continue,
                        }
                        match self.component_map.as_ref().unwrap().0.get(&ty.type_id()) {
                            None => continue,
                            Some(this) => {
                                if ui.button(name.clone()).clicked() {
                                    query.components.push((name, *this, ty.type_info().clone()));
                                    ui.close_menu();
                                }
                            }
                        }
                    }
                });
            }
        }
    }

    fn has_graph_menu(&mut self, pos: Pos2, snarl: &mut Snarl<ScriptNode>) -> bool {
        true
    }

    fn show_graph_menu(&mut self, pos: Pos2, ui: &mut Ui, scale: f32, snarl: &mut Snarl<ScriptNode>) {
        if ui.button("Query").clicked() {
            snarl.insert_node(pos, ScriptNode::query());
            ui.close_menu();
        }
        if ui.button("Set").clicked() {
            snarl.insert_node(pos, ScriptNode::set());
            ui.close_menu();
        }
        if ui.button("Field").clicked() {
            snarl.insert_node(pos, ScriptNode::field());
            ui.close_menu();
        }
        ui.menu_button("Functions", |ui| {
            ScrollArea::both().show(ui, |ui| {
                for (s, f) in self.function_registry.unwrap().0.iter() {
                    if ui.button(s).clicked() {
                        snarl.insert_node(
                            pos,
                            ScriptNode::function(f.info().clone().with_name(s.clone())),
                        );
                        ui.close_menu();
                    }
                }
            });
        });
        ui.menu_button("Type Creation", |ui| {
            ScrollArea::both().show(ui, |ui| {
                let mut nodes = vec![];
                let binding = self.type_registry.as_ref().unwrap().read();
                let binding2 = binding.iter();
                for ty in binding2 {
                    let Some(default) = ty.data::<ReflectDefault>() else {
                        continue;
                    };
                    match ty.type_info() {
                        TypeInfo::Struct(_) | TypeInfo::Value(_) => {}
                        _ => continue,
                    }
                    let name = remove_before_double_colon(ty.type_info().type_path());
                    nodes.push((name, default));
                }
                nodes.sort_by(|a, b| a.0.cmp(&b.0));
                for (name, default) in nodes {
                    if ui.button(name).clicked() {
                        snarl.insert_node(pos, ScriptNode::type_creation(default.default()));
                        ui.close_menu();
                    }
                }
            });
        });
    }

    fn has_node_menu(&mut self, node: &ScriptNode) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: NodeId,
        inputs: &[InPin],
        outputs: &[OutPin],
        ui: &mut Ui,
        scale: f32,
        snarl: &mut Snarl<ScriptNode>,
    ) {
        if ui.button("delete").clicked() {
            snarl.remove_node(node);
            ui.close_menu();
        }
        if ui.button("close").clicked() {
            ui.close_menu();
        }
    }
}
