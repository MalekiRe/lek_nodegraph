use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use bevy::ecs::component::ComponentId;
use bevy::prelude::Node;
use bevy::reflect::func::Arg;
use bevy::reflect::TypeInfo;
use egui_snarl::{InPinId, NodeId, OutPinId, Snarl};
use egui_snarl::ui::PinInfo;
use crate::scripting::{FieldNode, QueryNode, ScriptNode, SetNode, TypeCreationNode};
use crate::virtual_machine::Bytecode;


pub struct QueryComponent {
    name: String,
    id: ComponentId,
    info: TypeInfo,
}



pub fn compile(snarl: &Snarl<ScriptNode>) -> Vec<Bytecode> {
    // we gotta find the roots, rn i'm just gonna look for the query
    let mut query_n = None;
    for (node_id, node) in snarl.node_ids() {
        match node {
            ScriptNode::Query(_) => {
                query_n.replace(node_id);
            }
            _ => continue,
        }
    }

    let mut wire_stuff = WireStuff::default();
    for (out_pin_id, in_pin_id) in snarl.wires() {
        if !wire_stuff.input_map.contains_key(&in_pin_id.node) {
            wire_stuff.input_map.insert(in_pin_id.node, vec![]);
        }
        if !wire_stuff.output_map.contains_key(&out_pin_id.node) {
            wire_stuff.output_map.insert(out_pin_id.node, vec![]);
        }
        if !wire_stuff.pin_map.contains_key(&out_pin_id) {
            wire_stuff.pin_map.insert(out_pin_id, vec![]);
        }
        wire_stuff.input_map.get_mut(&in_pin_id.node).unwrap().push(in_pin_id);
        wire_stuff.output_map.get_mut(&out_pin_id.node).unwrap().push(out_pin_id);
        wire_stuff.pin_map.get_mut(&out_pin_id).unwrap().push(in_pin_id);
        wire_stuff.pin_map_2.insert(in_pin_id, out_pin_id);
    }
    let query_n = query_n.unwrap();

    let mut nodes_already_computed = HashSet::default();

    let mut tree = compute_data_flow(query_n, &mut nodes_already_computed, &snarl, &wire_stuff);

    let mut second_wire_stuff: SecondWireStuff = wire_stuff.into();

    let mut bytecode = vec![];
    let mut current_stack: usize = 0;
    loop {
        match tree.script_node {
            ScriptNode::Set(set_n) => set_node(tree.node_id, set_n, &mut second_wire_stuff, &mut bytecode, &mut current_stack),
            ScriptNode::Field(_) => todo!(),
            ScriptNode::Function(_) => todo!(),
            ScriptNode::TypeCreation(type_creation_n) => type_creation_node(tree.node_id, type_creation_n, &mut second_wire_stuff, &mut bytecode, &mut current_stack),
            ScriptNode::Query(query_n) => query_node(tree.node_id, query_n, &mut second_wire_stuff, &mut bytecode, &mut current_stack)
        }
        tree = match tree.left {
            None => break,
            Some(tree) => {
                *tree
            }
        }
    }

    bytecode

}

fn field_node(node_id: NodeId, field_node: FieldNode, wire_stuff: &mut SecondWireStuff, bytecode: &mut Vec<Bytecode>, current_stack: &mut usize) {
    todo!()
}

fn set_node(node_id: NodeId, set_node: SetNode, wire_stuff: &mut SecondWireStuff, bytecode: &mut Vec<Bytecode>, current_stack: &mut usize) {
    let field_set_id = InPinId {
        node: node_id,
        input: 1,
    };

    let field_get_id = InPinId {
        node: node_id,
        input: 2,
    };
    bytecode.push(Bytecode::Copy(wire_stuff.get_data_info(field_get_id).unwrap()));
    bytecode.push(Bytecode::SetField(wire_stuff.get_data_info(field_set_id).unwrap()));
    bytecode.push(Bytecode::Pop); // pop off what we copied to set.
}

fn type_creation_node(node_id: NodeId, type_creation_node: TypeCreationNode, wire_stuff: &mut SecondWireStuff, bytecode: &mut Vec<Bytecode>, current_stack: &mut usize) {
    let place_where_type_is_on_stack = OutPinId {
        node: node_id,
        output: 0,
    };
    wire_stuff.set_data_info(place_where_type_is_on_stack, *current_stack);
    *current_stack += 1;
    bytecode.push(Bytecode::Push(Arg::Owned(type_creation_node.value)));
}

fn query_node(node_id: NodeId, query_node: QueryNode, wire_stuff: &mut SecondWireStuff, bytecode: &mut Vec<Bytecode>, current_stack: &mut usize) {
    for i in 1..(query_node.components.len() + 1) {
        let query_component_output = OutPinId {
            node: node_id,
            output: i,
        };
        wire_stuff.set_data_info(query_component_output, *current_stack);
        *current_stack += 1;
    }
    bytecode.push(Bytecode::Query {
        components: query_node.components,
    });
}

fn compute_data_flow(node_id: NodeId, nodes_already_computed: &mut HashSet<NodeId>, snarl: &Snarl<ScriptNode>, wire_stuff: &WireStuff) -> TreeNode {
    nodes_already_computed.insert(node_id);
    let mut tree_node = TreeNode {
        node_id,
        script_node: snarl.get_node(node_id).unwrap().clone(),
        left: None,
    };

    let can_flow = snarl.get_node(node_id).unwrap().can_flow();

    for input_pin_id in wire_stuff.input_map.get(&node_id).unwrap_or(&vec![]) {
        if can_flow {
            if input_pin_id.input == 0 {
                continue;
            }
        }
        let out_pin = wire_stuff.pin_map_2.get(&input_pin_id).unwrap();
        if nodes_already_computed.contains(&out_pin.node) {
            continue;
        }
        nodes_already_computed.insert(out_pin.node);
        tree_node = compute_data_flow(out_pin.node, nodes_already_computed, snarl, wire_stuff).push_left(tree_node);
    }
    // TODO when we do if statements or any kind of branch we won't have can flow
    // We will branch based on flow and create each tree
    if can_flow {
        let next_node = match wire_stuff.output_map.get(&node_id) {
            None => return tree_node,
            Some(awa) => {
                awa.first().unwrap()
            }
        };
        let input = wire_stuff.pin_map.get(next_node).unwrap().first().unwrap().node;
        if nodes_already_computed.contains(&input) {
            return tree_node;
        }

        tree_node = tree_node.push_left(compute_data_flow(input, nodes_already_computed, snarl, wire_stuff));
        tree_node
        // walk down the zeroth node here last
    } else {
        tree_node
    }
}

#[derive(Debug)]
pub struct TreeNode {
    node_id: NodeId,
    script_node: ScriptNode,
    left: Option<Box<TreeNode>>,
}

impl TreeNode {
    pub fn push_left(mut self, new_node: TreeNode) -> TreeNode {
        match self.left.take() {
            None => {
                self.left.replace(Box::new(new_node));
            }
            Some(mut left) => {
                self.left.replace(Box::new(left.push_left(new_node)));
            }
        }
        self
    }
}

#[derive(Clone, Default)]
struct WireStuff {
    input_map: HashMap<NodeId, Vec<InPinId>>,
    output_map: HashMap<NodeId, Vec<OutPinId>>,
    pin_map: HashMap<OutPinId, Vec<InPinId>>,
    pin_map_2: HashMap<InPinId, OutPinId>,
}

#[derive(Clone, Default)]
struct SecondWireStuff {
    input_map: HashMap<NodeId, Vec<InPinId>>,
    output_map: HashMap<NodeId, Vec<OutPinId>>,
    pin_map: HashMap<OutPinId, Vec<InPinId>>,
    pin_map_2: HashMap<InPinId, OutPinId>,
    data_info: HashMap<OutPinId, usize>,
}

impl From<WireStuff> for SecondWireStuff {
    fn from(WireStuff { input_map, output_map, pin_map, pin_map_2 }: WireStuff) -> Self {
        SecondWireStuff {
            input_map,
            output_map,
            pin_map,
            pin_map_2,
            data_info: Default::default(),
        }
    }
}

pub trait Pin<T> {
    fn get_data_info(&self, pin: T) -> Option<usize>;
    fn has_data_info(&self, pin: T) -> bool;
    fn set_data_info(&mut self, pin: T, stack_position: usize);
}

impl Pin<OutPinId> for SecondWireStuff {
    fn get_data_info(&self, pin: OutPinId) -> Option<usize> {
        self.data_info.get(&pin).map(|a| *a)
    }

    fn has_data_info(&self, pin: OutPinId) -> bool {
        self.data_info.contains_key(&pin)
    }

    fn set_data_info(&mut self, pin: OutPinId, stack_position: usize) {
        self.data_info.insert(pin, stack_position);
    }
}

impl Pin<InPinId> for SecondWireStuff {
    fn get_data_info(&self, pin: InPinId) -> Option<usize> {
        self.get_data_info(*self.pin_map_2.get(&pin)?)
    }

    fn has_data_info(&self, pin: InPinId) -> bool {
        self.has_data_info(*self.pin_map_2.get(&pin).unwrap())
    }

    fn set_data_info(&mut self, pin: InPinId, stack_position: usize) {
        self.set_data_info(*self.pin_map_2.get(&pin).unwrap(), stack_position)
    }
}