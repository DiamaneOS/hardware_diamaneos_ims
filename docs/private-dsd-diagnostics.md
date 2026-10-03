# Private DSD notification observation

This temporary userdebug experiment counts well-framed response/indication headers
received from the runtime's already verified current modem endpoint. It exposes
only counts, the last indication message ID and a64-bin ID histogram through the existing owner-checked
status method. It retains no body, transaction, address, endpoint or subscriber
identity. Counters survive client reconnects and saturate rather than wrapping.

The Rust receive-path counters run only when immutable ro.debuggable is true;
property read failure disables them. User builds return zero/-1/empty and report
the disabled state explicitly. The existing observer logs only when Build.isDebuggable.
No indication body is accepted, no new request is sent, and protocol/state-machine
decisions are unchanged. In particular, this does not register measurement
indications, advertise support, measure signal or generate quality verdicts.

An indication header can identify an existing unhandled exchange for further
review. Its presence does not validate its body; its absence cannot rule out an
exchange that requires registration or capability negotiation. Remove this
temporary diagnostic surface after the interoperability failure is resolved.

The experiment starts from6332cb5, without the failed frequency/technology trials.
