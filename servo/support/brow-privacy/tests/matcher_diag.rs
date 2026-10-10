/* brow (7.2 diag): names the exact decision path for corpus URLs the
 * score rubric flags. Run in CI with --features brow-debug-decisions so
 * the engine's DBG lines (verify-fail / applies-fail / MATCH) print. */

use url::Url;

use brow_privacy::filter::rule::ResourceTypeMask;
use brow_privacy::lists;

#[test]
fn diagnose_facebook_tr() {
    let engine = lists::global_engine().unwrap();
    let site = Url::parse("https://www.example.com/").unwrap();
    let req = Url::parse("https://www.facebook.com/tr/?id=123456789&ev=PageView").unwrap();
    let decision = engine.should_block(&site, &req, ResourceTypeMask::XHR, true);
    println!("DIAG facebook.com/tr -> {decision:?}");

    // the easyprivacy rules we believe should match:
    // ||facebook.com/tr/ (line 11598), ||facebook.com/tr? (11599)
    assert!(
        matches!(decision, brow_privacy::filter::Decision::Block { .. }),
        "engine did not block https://www.facebook.com/tr/ — see DBG lines"
    );
}
