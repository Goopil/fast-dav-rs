//! CPU-bound hot-path benchmarks, measured by CodSpeed in CI.
//!
//! Unlike `benches/performance.rs` — which exercises the full client against
//! an in-process HTTP fixture — this suite calls the pure, synchronous
//! building blocks the client is made of: multistatus parsing, request-body
//! generation, iCalendar validation, item mapping and payload compression.
//! Everything runs without sockets, threads or timers, which makes the
//! measurements deterministic under CodSpeed's CPU simulation instrument.
//!
//! Payloads are generated once, outside the measured closures; only the
//! parse / build / map / compress call is timed.

use std::hint::black_box;

use bytes::Bytes;
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fast_dav_rs::caldav::{
    CalendarQueryFilter, PropFilter, TextMatch, TimeRange, build_calendar_multiget_body,
    build_calendar_query_body, map_calendar_objects, validate_icalendar,
};
use fast_dav_rs::carddav::{build_addressbook_multiget_body, map_address_objects};
use fast_dav_rs::common::compression::{ContentEncoding, compress_payload};
use fast_dav_rs::webdav::streaming::{parse_multistatus_bytes, parse_multistatus_bytes_visit};
use fast_dav_rs::webdav::{
    DavItem, escape_xml, parse_dav_header, parse_error_body, parse_lock_discovery_bytes,
};

/// One filler iCalendar line (~120 chars) used to size `calendar-data`
/// blobs. Kept XML-safe (no `<`, `&`, quotes).
const ICS_FILLER_LINE: &str = "DESCRIPTION:Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore 0123456789\n";

const SYNC_TOKEN: &str = "http://example.com/sync/bench";

/// Build a `207 Multi-Status` `sync-collection` body with `items` responses.
/// When `include_data` is set every item carries a `calendar-data` blob of
/// `data_lines` filler lines.
fn multistatus_payload(items: usize, include_data: bool, data_lines: usize) -> Vec<u8> {
    let mut body = String::with_capacity(items * (420 + data_lines * ICS_FILLER_LINE.len()));
    body.push_str(
        "<?xml version=\"1.0\"?>\n<D:multistatus xmlns:D=\"DAV:\" \
         xmlns:C=\"urn:ietf:params:xml:ns:caldav\">\n",
    );
    for i in 0..items {
        let n = i.to_string();
        body.push_str("  <D:response>\n    <D:href>/cal/event-");
        body.push_str(&n);
        body.push_str(
            ".ics</D:href>\n    <D:propstat>\n      <D:prop>\n        <D:getetag>\"etag-",
        );
        body.push_str(&n);
        body.push_str(
            "\"</D:getetag>\n        <D:getcontenttype>text/calendar</D:getcontenttype>\n",
        );
        if include_data {
            body.push_str(
                "        <C:calendar-data>BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//bench//EN\n",
            );
            for _ in 0..data_lines {
                body.push_str(ICS_FILLER_LINE);
            }
            body.push_str("END:VCALENDAR</C:calendar-data>\n");
        }
        body.push_str(
            "      </D:prop>\n      <D:status>HTTP/1.1 200 OK</D:status>\n    </D:propstat>\n  \
             </D:response>\n",
        );
    }
    body.push_str(&format!(
        "  <D:sync-token>{SYNC_TOKEN}</D:sync-token>\n</D:multistatus>\n"
    ));
    body.into_bytes()
}

/// A realistic VEVENT of `alarms` VALARM blocks and a folded description.
fn icalendar_payload(alarms: usize) -> Vec<u8> {
    let mut ics = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//bench//EN\r\nCALSCALE:GREGORIAN\r\n\
         BEGIN:VEVENT\r\nUID:bench-event-1@example.com\r\nDTSTAMP:20240101T090000Z\r\n\
         DTSTART:20240102T090000Z\r\nDTEND:20240102T100000Z\r\n\
         RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=104\r\nSUMMARY:Recurring planning meeting\r\n\
         DESCRIPTION:Lorem ipsum dolor sit amet consectetur adipiscing elit sed do\r\n \
         eiusmod tempor incididunt ut labore et dolore magna aliqua ut enim ad minim\r\n \
         veniam quis nostrud exercitation ullamco laboris\r\n",
    );
    for i in 0..alarms {
        ics.push_str(&format!(
            "BEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT{i}M\r\n\
             DESCRIPTION:Reminder {i}\r\nEND:VALARM\r\n"
        ));
    }
    ics.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
    ics.into_bytes()
}

fn hrefs(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("/cal/collection/event-{i}.ics"))
        .collect()
}

fn parsed_items(count: usize, include_data: bool) -> Vec<DavItem> {
    let payload = multistatus_payload(count, include_data, 4);
    let parsed = parse_multistatus_bytes(&payload).expect("parse multistatus fixture");
    assert_eq!(parsed.items.len(), count);
    parsed.items
}

/// Multistatus parsing — the dominant cost of every `PROPFIND`/`REPORT`
/// response. Aggregated (`Vec<DavItem>`) vs per-item visitor, etags-only vs
/// `calendar-data`-heavy bodies.
fn bench_multistatus_parse(c: &mut Criterion) {
    let cases = [
        ("200_etags_only", 200usize, false, 0usize),
        ("200_with_data", 200, true, 12),
        ("2000_etags_only", 2_000, false, 0),
        ("2000_with_data", 2_000, true, 12),
    ];

    let mut group = c.benchmark_group("multistatus_parse");
    for (label, items, include_data, data_lines) in cases {
        let payload = multistatus_payload(items, include_data, data_lines);
        group.throughput(Throughput::Bytes(payload.len() as u64));

        group.bench_function(BenchmarkId::new("aggregated", label), |b| {
            b.iter(|| {
                let parsed = parse_multistatus_bytes(black_box(&payload)).expect("parse");
                assert_eq!(parsed.items.len(), items);
                black_box(parsed.items.len())
            });
        });

        group.bench_function(BenchmarkId::new("visit", label), |b| {
            b.iter(|| {
                let mut seen = 0usize;
                let sync_token = parse_multistatus_bytes_visit(black_box(&payload), |item| {
                    seen += 1;
                    black_box(&item);
                    Ok(())
                })
                .expect("parse");
                assert_eq!(seen, items);
                assert_eq!(sync_token.as_deref(), Some(SYNC_TOKEN));
                black_box(seen)
            });
        });
    }
    group.finish();
}

/// Small-body parsers on the request path: `LOCK` responses, DAV error
/// bodies and the `DAV:` capability header.
fn bench_small_parsers(c: &mut Criterion) {
    const LOCK_BODY: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<D:prop xmlns:D="DAV:">
  <D:lockdiscovery>
    <D:activelock>
      <D:locktype><D:write/></D:locktype>
      <D:lockscope><D:exclusive/></D:lockscope>
      <D:depth>0</D:depth>
      <D:owner>https://example.com/alice</D:owner>
      <D:timeout>Second-3600</D:timeout>
      <D:locktoken><D:href>urn:uuid:8f5a4d1e-1d2b-4f3c-9d0a-1b2c3d4e5f60</D:href></D:locktoken>
      <D:lockroot><D:href>https://example.com/cal/event.ics</D:href></D:lockroot>
    </D:activelock>
  </D:lockdiscovery>
</D:prop>"#;

    const ERROR_BODY: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<D:error xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <C:no-uid-conflict><D:href>/cal/other-event.ics</D:href></C:no-uid-conflict>
  <D:responsedescription>An event with the same UID already exists</D:responsedescription>
</D:error>"#;

    const DAV_HEADER: &str = "1, 2, 3, access-control, calendar-access, calendar-auto-schedule, \
                              calendar-availability, addressbook, extended-mkcol, \
                              calendarserver-sharing, calendarserver-subscribed";

    let mut group = c.benchmark_group("small_parsers");
    group.bench_function("lock_discovery", |b| {
        b.iter(|| black_box(parse_lock_discovery_bytes(black_box(LOCK_BODY)).expect("lock")));
    });
    group.bench_function("error_body", |b| {
        b.iter(|| black_box(parse_error_body(black_box(ERROR_BODY)).expect("error body")));
    });
    group.bench_function("dav_header", |b| {
        b.iter(|| black_box(parse_dav_header(black_box(DAV_HEADER)).expect("dav header")));
    });
    group.finish();
}

/// Request-body generation: multiget bodies over large href batches, the
/// calendar-query builder and raw XML escaping.
fn bench_request_bodies(c: &mut Criterion) {
    let hrefs_1k = hrefs(1_000);
    let expand = TimeRange::new("20240101T000000Z").with_end("20241231T235959Z");
    let filter = CalendarQueryFilter::new("VEVENT")
        .with_time_range(TimeRange::new("20240101T000000Z").with_end("20240201T000000Z"))
        .with_prop_filters(vec![
            PropFilter::new("SUMMARY", TextMatch::new("weekly sync")),
            PropFilter::new("UID", TextMatch::new("bench-event-1@example.com")),
        ]);
    // ~64 KiB of text with a metacharacter in every chunk, i.e. the
    // worst case for the escaper.
    let escape_input = "Meeting <planning> & \"review\" — attendee's notes; ".repeat(1_300);

    let mut group = c.benchmark_group("request_bodies");
    group.bench_function("calendar_multiget_1k_hrefs", |b| {
        b.iter(|| {
            black_box(
                build_calendar_multiget_body(black_box(&hrefs_1k), true, None)
                    .expect("non-empty hrefs"),
            )
        });
    });
    group.bench_function("calendar_multiget_1k_hrefs_expand", |b| {
        b.iter(|| {
            black_box(
                build_calendar_multiget_body(black_box(&hrefs_1k), true, Some(&expand))
                    .expect("non-empty hrefs"),
            )
        });
    });
    group.bench_function("addressbook_multiget_1k_hrefs", |b| {
        b.iter(|| {
            black_box(
                build_addressbook_multiget_body(black_box(&hrefs_1k), true)
                    .expect("non-empty hrefs"),
            )
        });
    });
    group.bench_function("calendar_query", |b| {
        b.iter(|| {
            black_box(build_calendar_query_body(
                black_box("VEVENT"),
                Some("20240101T000000Z"),
                Some("20240201T000000Z"),
                true,
                None,
            ))
        });
    });
    group.bench_function("calendar_query_filter", |b| {
        b.iter(|| black_box(black_box(&filter).to_query_body(true)));
    });
    group.throughput(Throughput::Bytes(escape_input.len() as u64));
    group.bench_function("escape_xml_64kib", |b| {
        b.iter(|| black_box(escape_xml(black_box(&escape_input))));
    });
    group.finish();
}

/// iCalendar validation runs on every CalDAV `PUT`.
fn bench_icalendar_validation(c: &mut Criterion) {
    let small = icalendar_payload(0);
    let large = icalendar_payload(200);

    let mut group = c.benchmark_group("icalendar_validation");
    for (label, payload) in [("single_event", &small), ("200_alarms", &large)] {
        group.throughput(Throughput::Bytes(payload.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.iter(|| {
                validate_icalendar(black_box(payload)).expect("valid ics");
            });
        });
    }
    group.finish();
}

/// Mapping parsed `DavItem`s into the typed CalDAV/CardDAV views returned to
/// callers.
fn bench_item_mapping(c: &mut Criterion) {
    let calendar_items = parsed_items(1_000, true);
    let address_items = parsed_items(1_000, false);

    let mut group = c.benchmark_group("item_mapping");
    group.bench_function("map_calendar_objects_1k", |b| {
        b.iter_batched(
            || calendar_items.clone(),
            |items| black_box(map_calendar_objects(items).len()),
            BatchSize::SmallInput,
        );
    });
    group.bench_function("map_address_objects_1k", |b| {
        b.iter_batched(
            || address_items.clone(),
            |items| black_box(map_address_objects(items).len()),
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

/// Request-body compression, applied to outgoing REPORT bodies when the
/// server advertises support for it.
fn bench_request_compression(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("tokio runtime");
    // A realistic multiget body: highly repetitive XML, ~90 KiB.
    let payload = Bytes::from(
        build_calendar_multiget_body(hrefs(1_500), true, None).expect("non-empty hrefs"),
    );

    let mut group = c.benchmark_group("request_compression");
    group.throughput(Throughput::Bytes(payload.len() as u64));
    for (label, encoding) in [
        ("gzip", ContentEncoding::Gzip),
        ("zstd", ContentEncoding::Zstd),
        ("br", ContentEncoding::Br),
    ] {
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.to_async(&rt).iter(|| {
                let payload = payload.clone();
                async move {
                    let compressed = compress_payload(payload, encoding)
                        .await
                        .expect("compress payload");
                    black_box(compressed.len())
                }
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_multistatus_parse,
    bench_small_parsers,
    bench_request_bodies,
    bench_icalendar_validation,
    bench_item_mapping,
    bench_request_compression,
);
criterion_main!(benches);
