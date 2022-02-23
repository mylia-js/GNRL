use crate::obj_props;
use crate::optimize::create_spawn_trigger;
use crate::optimize::is_start_group;
use crate::optimize::replace_groups;
use crate::optimize::ToggleGroups;
use crate::ReservedIds;
use crate::Trigger;
use crate::TriggerNetwork;
use crate::TriggerRole;
use crate::Triggerlist;
use crate::NO_GROUP;
use compiler::builtins::Group;
use compiler::leveldata::ObjParam;
use fnv::FnvHashMap;
use fnv::FnvHashSet;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
struct SpawnDelay {
    pub(crate) delay: u32,
    pub(crate) epsiloned: bool,
}

struct Connection {
    start_group: Group,
    end_group: Group,
    delay: SpawnDelay,
    trigger: Trigger,
}

struct SpawnTrigger {
    target: Group,
    delay: SpawnDelay,
    trigger: Trigger,
}

// fn can_toggle_on(obj: &GdObj) -> bool {
//     if let Some(ObjParam::Number(obj_id)) = obj.params.get(&1) {
//         match *obj_id as u16 {
//             obj_ids::TOUCH
//             | obj_ids::COUNT
//             | obj_ids::INSTANT_COUNT
//             | obj_ids::COLLISION
//             | obj_ids::ON_DEATH => {
//                 if let Some(ObjParam::Bool(false)) | None =
//                     obj.params.get(&obj_props::ACTIVATE_GROUP)
//                 {
//                     false
//                 } else {
//                     matches!(obj.params.get(&obj_props::TARGET), Some(_))
//                 }
//             }
//             _ => false,
//         }
//     } else {
//         false
//     }
// }

// spawn trigger optimisation
pub(crate) fn spawn_optimisation(
    network: &mut TriggerNetwork,
    objects: &mut Triggerlist,
    reserved: &ReservedIds,
    toggle_groups: &ToggleGroups,
) {
    let mut spawn_connections = FnvHashMap::<Group, Vec<SpawnTrigger>>::default();
    let mut inputs = FnvHashSet::<Group>::default();
    let mut outputs = FnvHashSet::<Group>::default();

    let mut cycle_points = FnvHashSet::<Group>::default();
    let mut all = Vec::new();

    for (group, gang) in network.iter_mut() {
        let output_condition = gang.triggers.iter().any(|t| t.role != TriggerRole::Spawn);
        if output_condition {
            outputs.insert(*group);
        }
        for trigger in &mut gang.triggers {
            let obj = &objects[trigger.obj].0.params;

            if trigger.role == TriggerRole::Spawn {
                // dont include ones that dont activate a group

                let target = match obj.get(&obj_props::TARGET) {
                    Some(ObjParam::Group(g)) => *g,

                    _ => continue,
                };

                if gang.non_spawn_triggers_in || *group == NO_GROUP {
                    inputs.insert(*group);
                }

                let delay = match obj.get(&63).unwrap_or(&ObjParam::Number(0.0)) {
                    ObjParam::Number(d) => SpawnDelay {
                        delay: (*d * 1000.0) as u32,
                        epsiloned: false,
                    },
                    ObjParam::Epsilon => SpawnDelay {
                        delay: 0,
                        epsiloned: true,
                    },
                    _ => SpawnDelay {
                        delay: 0,
                        epsiloned: false,
                    },
                };

                // delete trigger that will be rebuilt
                (*trigger).deleted = true;

                if let Some(l) = spawn_connections.get_mut(group) {
                    l.push(SpawnTrigger {
                        target,
                        delay,
                        trigger: *trigger,
                    })
                } else {
                    spawn_connections.insert(
                        *group,
                        vec![SpawnTrigger {
                            target,
                            delay,
                            trigger: *trigger,
                        }],
                    );
                }
            }
        }
    }

    for start in inputs.clone() {
        let mut visited = Vec::new();
        look_for_cycle(
            start,
            &spawn_connections,
            &mut visited,
            &mut inputs,
            &mut outputs,
            &mut cycle_points,
            &mut all,
        )
    }

    // println!(
    //     "spawn_triggers: {:?}\n\n inputs: {:?}\n\n outputs: {:?}\n",
    //     spawn_connections, inputs, outputs
    // );

    // go from every trigger in an input group and get every possible path to an
    // output group (stopping if it reaches a group already visited)

    for start in inputs {
        //println!("<{:?}>", start);
        let mut visited = Vec::new();
        traverse(
            start,
            start,
            SpawnDelay {
                delay: 0,
                epsiloned: false,
            },
            None,
            &outputs,
            &cycle_points,
            &spawn_connections,
            &mut visited,
            &mut all,
        );
        //println!("</{:?}>", start);
    }

    let mut deduped = FnvHashMap::default();

    for Connection {
        start_group,
        end_group,
        delay,
        trigger,
    } in all
    {
        deduped.insert((start_group, end_group, delay), trigger);
    }

    let mut swaps = FnvHashMap::default();

    let mut insert_to_swaps = |a: Group, b: Group| {
        for v in swaps.values_mut() {
            if *v == a {
                *v = b;
            }
        }
        assert!(swaps.insert(a, b).is_none());
    };
    // let mut start_counts = FnvHashMap::default();
    // let mut end_counts = FnvHashMap::default();

    // for ((start, end, _), _) in deduped.iter() {
    //     start_counts
    //         .entry(start)
    //         .and_modify(|c| *c += 1)
    //         .or_insert(1);

    //     end_counts.entry(end).and_modify(|c| *c += 1).or_insert(1);
    // }

    for ((start, end, delay), trigger) in deduped {
        let d = if delay.delay < 50 && delay.epsiloned {
            50
        } else {
            delay.delay
        };
        let mut plain_trigger = |network| {
            create_spawn_trigger(
                trigger,
                end,
                start,
                d as f64 / 1000.0,
                objects,
                network,
                TriggerRole::Spawn,
                false,
            )
        };
        if toggle_groups.toggles_off.contains(&start)
            || (toggle_groups.toggles_on.contains(&start)
                && toggle_groups.toggles_off.contains(&end))
        {
            plain_trigger(network)
        } else if d == 0 && !is_start_group(end, reserved) && network[&end].connections_in == 1 {
            //dbg!(end, start);
            insert_to_swaps(end, start);
        } else if d == 0 && !is_start_group(start, reserved)
                && network[&start].connections_in == 1 //??
                && (network[&start].triggers.is_empty()
                    || network[&start].triggers.iter().all(|t| t.deleted))
        {
            //dbg!(start, end);
            insert_to_swaps(start, end);
        } else {
            plain_trigger(network)
        }
    }

    replace_groups(swaps, objects);
}

// set triggers that make cycles to inputs and outputs
fn look_for_cycle(
    current: Group,
    ictriggers: &FnvHashMap<Group, Vec<SpawnTrigger>>,
    visited: &mut Vec<Group>,
    inputs: &mut FnvHashSet<Group>,
    outputs: &mut FnvHashSet<Group>,
    cycle_points: &mut FnvHashSet<Group>,
    all: &mut Vec<Connection>,
) {
    if let Some(connections) = ictriggers.get(&current) {
        for SpawnTrigger {
            target: g,
            delay,
            trigger,
        } in connections
        {
            if visited.contains(g) {
                //println!("cycle detected");
                outputs.insert(current);
                inputs.insert(*g);
                all.push(Connection {
                    start_group: current,
                    end_group: *g,
                    delay: *delay,
                    trigger: *trigger,
                });
                cycle_points.insert(current);

                return;
            }

            visited.push(current);

            look_for_cycle(*g, ictriggers, visited, inputs, outputs, cycle_points, all);

            assert_eq!(visited.pop(), Some(current));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn traverse(
    current: Group,
    origin: Group,
    total_delay: SpawnDelay, // delay from the origin to the current trigger
    trigger: Option<Trigger>,
    outputs: &FnvHashSet<Group>,
    cycle_points: &FnvHashSet<Group>,
    spawn_connections: &FnvHashMap<Group, Vec<SpawnTrigger>>,
    visited: &mut Vec<Group>,
    all: &mut Vec<Connection>,
) {
    if visited.contains(&current) {
        unreachable!()
    }

    if let Some(connections) = spawn_connections.get(&current) {
        for SpawnTrigger {
            target: g,
            delay: d,
            trigger,
        } in connections
        {
            //println!("{:?} -> {:?}", current, g);
            let new_delay = SpawnDelay {
                delay: total_delay.delay + d.delay,
                epsiloned: total_delay.epsiloned || d.epsiloned,
            };
            visited.push(current);
            if outputs.contains(g) {
                all.push(Connection {
                    start_group: origin,
                    end_group: *g,
                    delay: new_delay,
                    trigger: *trigger,
                });

                // avoid infinite loop
                if !cycle_points.contains(g) {
                    traverse(
                        *g,
                        *g,
                        SpawnDelay {
                            delay: 0,
                            epsiloned: false,
                        },
                        None,
                        outputs,
                        cycle_points,
                        spawn_connections,
                        visited,
                        all,
                    );
                }
            } else {
                traverse(
                    *g,
                    origin,
                    new_delay,
                    Some(*trigger),
                    outputs,
                    cycle_points,
                    spawn_connections,
                    visited,
                    all,
                );
            }
            assert_eq!(visited.pop(), Some(current));
        }
    } else if let Some(t) = trigger {
        all.push(Connection {
            start_group: origin,
            end_group: current,
            delay: total_delay,
            trigger: t,
        }) //?
    } else {
        //unreachable!();
        assert!(outputs.contains(&current));
    }
}
