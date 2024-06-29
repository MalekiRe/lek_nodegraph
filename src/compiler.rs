use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use bevy::ecs::component::ComponentId;
use bevy::prelude::Node;
use bevy::reflect::func::Arg;
use bevy::reflect::TypeInfo;
use egui_snarl::{InPinId, NodeId, OutPinId, Snarl};
use egui_snarl::ui::PinInfo;
use crate::indirect_stack::StackValue;
use crate::scripting::{FieldNode, FunctionNode, QueryNode, ScriptNode, SetNode, TypeCreationNode};
use crate::virtual_machine::Bytecode;

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

    //println!("{:#?}", tree);

    let mut second_wire_stuff: SecondWireStuff = wire_stuff.into();

    let mut bytecode = vec![];
    let mut current_stack: usize = 0;
    loop {
        match tree.script_node {
            ScriptNode::Set(set_n) => set_node(tree.node_id, set_n, &mut second_wire_stuff, &mut bytecode, &mut current_stack),
            ScriptNode::Field(field_n) => field_node(tree.node_id, field_n, &mut second_wire_stuff, &mut bytecode, &mut current_stack),
            ScriptNode::Function(function_n) => function_node(tree.node_id, function_n, &mut second_wire_stuff, &mut bytecode, &mut current_stack),
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


fn function_node(node_id: NodeId, function_node: FunctionNode, wire_stuff: &mut SecondWireStuff, bytecode: &mut Vec<Bytecode>, current_stack: &mut usize) {
    // skip flow node
    for i in 1..(function_node.function_info.arg_count()+1) {
        let arg_node = wire_stuff.get_data_info(InPinId {
            node: node_id,
            input: i,
        }).unwrap();
        bytecode.push(Bytecode::Copy(arg_node));
        // we don't have to increase the stack because we are about to pop it all off for the function
    }
    bytecode.push(Bytecode::Call(function_node.function_info.name().unwrap().to_string()));

}
fn field_node(node_id: NodeId, field_node: FieldNode, wire_stuff: &mut SecondWireStuff, bytecode: &mut Vec<Bytecode>, current_stack: &mut usize) {
    let struct_node = wire_stuff.get_data_info(InPinId {
        node: node_id,
        input: 0,
    }).unwrap();

    wire_stuff.set_data_info(OutPinId {
        node: node_id,
        output: 0,
    }, *current_stack);

    let name = field_node.name.unwrap();
    bytecode.push(Bytecode::GetField(struct_node, name));
    *current_stack += 1;

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
    // what we copied to set gets automatically popped off what we copied to set.
}

fn type_creation_node(node_id: NodeId, type_creation_node: TypeCreationNode, wire_stuff: &mut SecondWireStuff, bytecode: &mut Vec<Bytecode>, current_stack: &mut usize) {
    let place_where_type_is_on_stack = OutPinId {
        node: node_id,
        output: 0,
    };
    wire_stuff.set_data_info(place_where_type_is_on_stack, *current_stack);
    *current_stack += 1;
    bytecode.push(Bytecode::Push(StackValue::Owned(type_creation_node.value)));
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
    let tree_node = compute_data(node_id, nodes_already_computed, snarl, wire_stuff);

    let can_flow = snarl.get_node(node_id).unwrap().can_flow();

    if !can_flow {
        panic!("in compute data flow but can't flow")
    }

    let output_flow = OutPinId {
        node: node_id,
        output: 0,
    };

    match wire_stuff.pin_map.get(&output_flow).unwrap_or(&vec![]).first() {
        None => {
            tree_node
        }
        Some(next_flow) => {
            let next_flow = next_flow.node;
            let next_to_flow = compute_data_flow(next_flow, nodes_already_computed, snarl, wire_stuff);
            tree_node.push_left(next_to_flow)
        }
    }
}

fn compute_data(node_id: NodeId, nodes_already_computed: &mut HashSet<NodeId>, snarl: &Snarl<ScriptNode>, wire_stuff: &WireStuff) -> TreeNode {

    nodes_already_computed.insert(node_id);

    let mut tree_node = TreeNode {
        node_id,
        script_node: snarl.get_node(node_id).unwrap().clone(),
        left: None,
    };

    let can_flow = snarl.get_node(node_id).unwrap().can_flow();

    let mut previous_dependency_node = None;

    for input in wire_stuff.input_map.get(&node_id).unwrap_or(&vec![]) {
        if can_flow && input.input == 0 {
            continue; //skip flow nodes
        }
        let output_pin_id = wire_stuff.pin_map_2.get(input).unwrap();
        let data_node_dependency = output_pin_id.node;
        if nodes_already_computed.contains(&data_node_dependency) {
            continue;
        }
        let dependency_tree = compute_data(data_node_dependency, nodes_already_computed, snarl, wire_stuff);
        if previous_dependency_node.is_none() {
            previous_dependency_node.replace(dependency_tree);
        } else {
            let mut temp = previous_dependency_node.take().unwrap();
            previous_dependency_node.replace(temp.push_left(dependency_tree));
        }
    }

    match previous_dependency_node {
        None => {
            tree_node
        }
        Some(previous_dependency) => {
            previous_dependency.push_left(tree_node)
        }
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