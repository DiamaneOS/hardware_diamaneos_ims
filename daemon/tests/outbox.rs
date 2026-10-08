// SPDX-License-Identifier: Apache-2.0
use diamaneos_ims_dcm::engine::{Peer, MAX_SESSIONS};
use diamaneos_ims_runtime::outbox::{Attempt, Finished, Outbox};
const A: Peer = Peer { node: 3, port: 100 };
const B: Peer = Peer { node: 3, port: 101 };

#[test]
fn blocked_head_keeps_fifo_without_starving_another_peer() {
    let mut queue = Outbox::default();
    assert!(queue.enqueue(A, 1, 20, 0).is_ok());
    assert!(queue.enqueue(A, 2, 20, 0).is_ok());
    assert!(queue.enqueue(B, 3, 20, 0).is_ok());
    assert!(queue
        .attempt(
            1,
            |_, _| true,
            |peer, value| {
                assert_eq!(peer, A);
                assert_eq!(*value, 1);
                Attempt::Backpressured
            }
        )
        .is_none());
    assert!(matches!(
        queue.attempt(
            2,
            |_, _| true,
            |peer, value| {
                assert_eq!(peer, B);
                assert_eq!(*value, 3);
                Attempt::Submitted
            }
        ),
        Some(Finished::Submitted(3))
    ));
    assert!(matches!(
        queue.attempt(
            3,
            |_, _| true,
            |_, value| {
                assert_eq!(*value, 1);
                Attempt::Submitted
            }
        ),
        Some(Finished::Submitted(1))
    ));
    assert!(matches!(
        queue.attempt(
            4,
            |_, _| true,
            |_, value| {
                assert_eq!(*value, 2);
                Attempt::Submitted
            }
        ),
        Some(Finished::Submitted(2))
    ));
    assert_eq!(queue.peer_count(), 0);
}

#[test]
fn expiry_and_overflow_return_all_affected_ownership_only() {
    let mut queue = Outbox::default();
    let capacity = 4 * MAX_SESSIONS;
    for i in 0..capacity {
        assert!(queue.enqueue(A, i, 20, 0).is_ok());
    }
    let rejected = match queue.enqueue(A, capacity, 20, 0) {
        Err(value) => value,
        _ => panic!(),
    };
    assert_eq!(rejected.token, capacity);
    assert!(queue.enqueue(B, 200, 20, 1000).is_ok());
    match queue.attempt(
        2000,
        |_, _| true,
        |_, _| panic!("expired head must not submit"),
    ) {
        Some(Finished::Expired(peer, values)) => {
            assert_eq!(peer, A);
            assert_eq!(values.len(), capacity);
        }
        _ => panic!(),
    }
    assert_eq!(queue.peer_count(), 1);
    assert_eq!(queue.drain(), vec![200]);
}

#[test]
fn terminal_results_keep_room_in_a_full_queue_and_their_order() {
    let mut queue = Outbox::default();
    let capacity = 4 * MAX_SESSIONS;
    for i in 0..capacity {
        assert!(queue.enqueue(A, i, 20, 0).is_ok());
    }
    for i in 0..MAX_SESSIONS {
        assert!(queue.enqueue_terminal(A, capacity + i, 20, 0).is_ok());
    }
    // The reserve is for terminal results only, and bounded.
    assert!(queue.enqueue(A, 999, 20, 0).is_err());
    assert!(queue.enqueue_terminal(A, 999, 20, 0).is_err());
    let values = queue.purge(A);
    assert_eq!(values, (0..capacity + MAX_SESSIONS).collect::<Vec<_>>());
}

#[test]
fn only_a_failed_send_reports_peer_loss_and_expired_work_can_be_queued_again() {
    let mut queue = Outbox::default();
    assert!(queue.enqueue(A, 1, 20, 0).is_ok());
    assert!(queue.enqueue(A, 2, 20, 0).is_ok());
    let values = match queue.attempt(2000, |_, _| true, |_, _| panic!("expired")) {
        Some(Finished::Expired(A, values)) => values,
        _ => panic!(),
    };
    for value in values {
        assert!(queue.enqueue(A, value, 20, 2000).is_ok());
    }
    assert!(queue
        .attempt(3999, |_, _| true, |_, _| Attempt::Backpressured)
        .is_none());
    assert!(matches!(
        queue.attempt(3999, |_, _| true, |_, _| Attempt::PeerLost),
        Some(Finished::PeerLost(A, values)) if values == vec![1, 2]
    ));
    assert_eq!(queue.peer_count(), 0);
}

#[test]
fn obsolete_head_is_consumed_once_without_submitting_successor_in_same_turn() {
    let mut queue = Outbox::default();
    assert!(queue.enqueue(A, 1, 20, 0).is_ok());
    assert!(queue.enqueue(A, 2, 20, 0).is_ok());
    assert!(matches!(
        queue.attempt(1, |_, _| true, |_, _| Attempt::Obsolete),
        Some(Finished::Obsolete(1))
    ));
    assert_eq!(queue.purge(A), vec![2]);
    assert!(queue.purge(A).is_empty());
}

#[test]
fn replaced_networks_release_obsolete_capacity_without_reordering_live_work() {
    let mut queue = Outbox::default();
    assert!(queue.enqueue(A, (0, "reply"), 20, 0).is_ok());
    assert!(queue.enqueue(A, (1, "up"), 20, 0).is_ok());
    assert!(queue.enqueue(A, (0, "release"), 0, 0).is_ok());
    assert!(queue.enqueue(A, (2, "up"), 20, 1).is_ok());
    let removed = queue.discard_obsolete(A, |&(generation, kind)| kind != "up" || generation == 2);
    assert_eq!(removed, vec![(1, "up")]);
    assert_eq!(queue.drain(), vec![(0, "reply"), (0, "release"), (2, "up")]);
}

#[test]
fn an_expired_obsolete_head_does_not_retire_a_fresh_successor() {
    let mut queue = Outbox::default();
    assert!(queue.enqueue(A, 1, 20, 0).is_ok());
    assert!(queue.enqueue(A, 2, 20, 1000).is_ok());
    assert!(matches!(
        queue.attempt(
            2000,
            |_, value| *value != 1,
            |_, _| panic!("obsolete packet must not submit")
        ),
        Some(Finished::Obsolete(1))
    ));
    assert!(matches!(
        queue.attempt(
            2001,
            |_, _| true,
            |_, value| {
                assert_eq!(*value, 2);
                Attempt::Submitted
            }
        ),
        Some(Finished::Submitted(2))
    ));
    assert_eq!(queue.peer_count(), 0);
}
