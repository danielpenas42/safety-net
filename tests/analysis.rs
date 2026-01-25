use safety_net::CombDepthResult;
use safety_net::FanOutTable;
use safety_net::Gate;
use safety_net::GateNetlist;
use safety_net::Net;
use safety_net::Netlist;
use safety_net::SimpleCombDepth;
use safety_net::dont_touch_filter;
use safety_net::format_id;
use safety_net::iter::DFSIterator;
use std::rc::Rc;

fn and_gate() -> Gate {
    Gate::new_logical("AND".into(), vec!["A".into(), "B".into()], "Y".into())
}

fn reg() -> Gate {
    Gate::new_logical("REG".into(), vec!["D".into()], "Q".into())
}

fn get_simple_example() -> Rc<GateNetlist> {
    let netlist = Netlist::new("example".to_string());

    let a = netlist.insert_input("a".into());
    let b = netlist.insert_input("b".into());

    let instance = netlist
        .insert_gate(and_gate(), "inst_0".into(), &[a, b])
        .unwrap();

    instance.expose_with_name("y".into());

    netlist
}

fn divider_netlist() -> Rc<GateNetlist> {
    let netlist = Netlist::new("example".to_string());

    // Add the the input
    let a = netlist.insert_input("a".into());

    // Instantiate a reg
    let reg = netlist.insert_gate_disconnected(reg(), "inst_0".into());

    // And last val and input
    let and = netlist
        .insert_gate(and_gate(), "inst_1".into(), &[a, reg.get_output(0)])
        .unwrap();

    reg.find_input(&"D".into()).unwrap().connect(and.into());

    // Make this Reg an output
    reg.expose_with_name("y".into());

    netlist
}

#[test]
fn test_detect_cycles() {
    let netlist = get_simple_example();

    // No cycles yet.
    let dfs_iter = DFSIterator::new(&netlist, netlist.last().unwrap());
    assert!(!dfs_iter.detect_cycles());

    let input = netlist.inputs().next().unwrap();
    let inverter = Gate::new_logical("INV".into(), vec!["I".into()], "O".into());
    let inverted = netlist
        .insert_gate(inverter, "inst_0".into(), std::slice::from_ref(&input))
        .unwrap();
    assert!(netlist.replace_net_uses(input, &inverted.into()).is_ok());

    // Now there is a cycle.
    // We replaced the inverter input with invert output.
    // Simple combinational loop.
    let dfs_iter = DFSIterator::new(&netlist, netlist.last().unwrap());
    assert!(dfs_iter.detect_cycles());
}

#[test]
fn test_detect_cycles2() {
    let netlist = divider_netlist();

    let dfs_iter = DFSIterator::new(&netlist, netlist.last().unwrap());
    assert!(dfs_iter.detect_cycles());
}

#[test]
fn test_attr_filter() {
    let netlist = GateNetlist::new("example".to_string());

    let a: Vec<_> = (0..4)
        .map(|i| netlist.insert_input(Net::new_logic(format_id!("input_{}", i))))
        .collect::<Vec<_>>();

    let inst_0 = netlist
        .insert_gate(and_gate(), "inst_0".into(), &[a[0].clone(), a[1].clone()])
        .unwrap();

    let inst_1 = netlist
        .insert_gate(and_gate(), "inst_1".into(), &[a[1].clone(), a[2].clone()])
        .unwrap();

    netlist
        .insert_gate(
            and_gate(),
            "inst_2".into(),
            &[inst_0.into(), inst_1.clone().into()],
        )
        .unwrap();

    inst_1.set_attribute("dont_touch".into());
    for dt in dont_touch_filter(&*netlist) {
        assert!(dt == inst_1);
    }

    let filter = dont_touch_filter(&*netlist);

    for obj in netlist.objects() {
        if obj == inst_1 {
            assert!(filter.has(&obj));
        } else {
            assert!(!filter.has(&obj));
        }
    }

    assert_eq!(filter.keys().len(), 1)
}

#[cfg(feature = "graph")]
#[test]
fn test_petgraph() {
    use safety_net::MultiDiGraph;

    let netlist = get_simple_example();

    let petgraph = netlist.get_analysis::<MultiDiGraph<_>>();
    assert!(petgraph.is_ok());
    let petgraph = petgraph.unwrap();
    let graph = petgraph.get_graph();
    // Outputs are a pseudo node
    assert_eq!(graph.node_count(), 4);
    assert_eq!(graph.edge_count(), 3);
}

#[test]
fn test_comb_depth() {
    let netlist = get_simple_example();
    let depth_info = netlist.get_analysis::<SimpleCombDepth<_>>();
    assert!(depth_info.is_ok());
    let depth_info = depth_info.unwrap();

    let gate = netlist.last().unwrap();

    assert_eq!(
        depth_info.get_comb_depth(&gate),
        &CombDepthResult::Depth((1))
    );
    assert_eq!(depth_info.get_max_depth(), Some(1));
}

#[test]
fn test_comb_depth_dag_shared_subgraph() {
    let netlist = Netlist::new("dag".to_string());

    let a = netlist.insert_input("a".into());
    let b = netlist.insert_input("b".into());
    let c = netlist.insert_input("c".into());

    let and = netlist
        .insert_gate(and_gate(), "and".into(), &[a.clone(), b.clone()])
        .unwrap();

    let or = netlist
        .insert_gate(
            Gate::new_logical("OR".into(), vec!["A".into(), "B".into()], "Y".into()),
            "or".into(),
            &[and.clone().into(), c.clone()],
        )
        .unwrap();

    or.expose_with_name("y".into());
    let or_node = netlist.last().unwrap();

    let depth_info = netlist.get_analysis::<SimpleCombDepth<_>>().unwrap();

    assert_eq!(depth_info.get_comb_depth(&and), &CombDepthResult::Depth(1));
    assert_eq!(
        depth_info.get_comb_depth(&or_node),
        &CombDepthResult::Depth(2)
    );
    assert_eq!(depth_info.get_max_depth(), Some(2));
}

#[test]
fn test_comb_depth_incomplete() {
    let netlist = Netlist::new("incomplete".to_string());

    let a = netlist.insert_input("a".into());

    // Create AND gate but do NOT connect all inputs
    let and = netlist.insert_gate_disconnected(and_gate(), "and".into());

    // Connect only one input
    and.find_input(&"A".into()).unwrap().connect(a.into());
    // "B" is left unconnected → incomplete

    and.expose_with_name("y".into());

    let depth_info = netlist.get_analysis::<SimpleCombDepth<_>>().unwrap();
    let and_node = netlist.last().unwrap();
    assert_eq!(
        depth_info.get_comb_depth(&and_node),
        &CombDepthResult::Undefined
    );
}

#[test]
fn test_comb_depth_cycle() {
    let netlist = Netlist::new("cycle".to_string());

    let inv = netlist.insert_gate_disconnected(
        Gate::new_logical("INV".into(), vec!["I".into()], "O".into()),
        "inv".into(),
    );

    let input = inv.find_input(&"I".into()).unwrap();
    let output = inv.get_output(0);

    // Create combinational loop
    input.connect(output.into());

    inv.expose_with_name("y".into());

    let depth_info = netlist.get_analysis::<SimpleCombDepth<_>>().unwrap();
    let inv_node = netlist.last().unwrap();

    assert_eq!(
        depth_info.get_comb_depth(&inv_node),
        &CombDepthResult::PartOfCycle
    );
}

#[test]
fn test_fanout_table() {
    let netlist = get_simple_example();
    let fanout_table = netlist.get_analysis::<FanOutTable<_>>();
    assert!(fanout_table.is_ok());
    let fanout_table = fanout_table.unwrap();
    let gate = netlist.last().unwrap();
    // Outputs don't have users that are nodes
    assert_eq!(fanout_table.get_node_users(&gate).count(), 0);
}
