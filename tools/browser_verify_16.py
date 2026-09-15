"""Browser verify: bench page in real Chromium via Playwright (pip install playwright).

Flow: serve demo/ (`python3 -m http.server 8080 --directory demo`),
load index.html, boot the blinky preset (BOOT/BLINK markers via the
UART box), run the BLE self-test, run the 16 depth probes, assert
16/16 with zero page errors. Prints a one-line verdict per stage and
exits nonzero on any mismatch.

Run:  python3 tools/browser_verify_16.py  (bench must be served on :8080)
"""
import sys
from playwright.sync_api import sync_playwright

URL = 'http://127.0.0.1:8080/index.html'

def main():
    errors = []
    with sync_playwright() as p:
        browser = p.chromium.launch(args=['--no-sandbox'])
        page = browser.new_page()
        page.on('pageerror', lambda e: errors.append(str(e)))
        page.goto(URL)
        page.wait_for_timeout(6000)
        # 1. blinky preset boot: selecting the preset STAGES it (same
        # as Load); Run boots. Expect BOOT/BLINK markers in the UART box.
        page.select_option('#preset', 'blinky')
        page.wait_for_timeout(1000)
        page.click('#fwrun')
        page.wait_for_timeout(12000)
        uart = page.eval_on_selector('#uart', 'el => el.value')
        mips = page.eval_on_selector('#mips', 'el => el.textContent')
        ok_boot = 'BOOT' in uart and 'BLINK' in uart
        print(f'boot: {"OK" if ok_boot else "FAIL"} uart={uart[-60:]!r} mips={mips!r}')
        if not ok_boot:
            print('BROWSER FAIL: blinky markers missing'); return 1
        # 2. BLE self-test (loopback, no bridge needed)
        page.click('#bleSelftest')
        page.wait_for_timeout(8000)
        st = page.eval_on_selector('#bleSelftestStatus', 'el => el.textContent')
        ok_st = st.startswith('pass:')
        print(f'selftest: {"OK" if ok_st else "FAIL"} {st!r}')
        if not ok_st:
            print('BROWSER FAIL: BLE self-test'); return 1
        # 3. depth probes: 16/16
        page.click('#depthRun')
        page.wait_for_timeout(20000)
        depth = page.eval_on_selector('#depthStatus', 'el => el.textContent')
        rows = page.eval_on_selector_all(
            '#depthTable tbody tr',
            'els => els.map(el => el.textContent)')
        npass = sum(1 for r in rows if 'pass' in r.split('fail')[0])
        print(f'depth: {depth!r} ({npass}/{len(rows)} rows pass)')
        for r in rows:
            print('   ', r[:110])
        if depth != '16/16 probes pass':
            print('BROWSER FAIL: depth probes'); return 1
        print('pageerrors:', errors if errors else 'none')
        if errors:
            print('BROWSER FAIL: page errors'); return 1
        browser.close()
    print('BROWSER 16/16 OK')
    return 0

sys.exit(main())
