"""Headless-Chrome tests for feyonics-studio-web.html with mocked Web Bluetooth / Web MIDI / Web Serial.
Run:  python3 -m http.server 8765 (in a folder holding index.html = the studio page and feyonics-studio.html = the MIDI editor)
      python3 tests/test_web.py http://localhost:8765/
"""
import sys, json
from playwright.sync_api import sync_playwright

URL = sys.argv[1] if len(sys.argv) > 1 else 'http://localhost:8765/'
SHOT = sys.argv[2] if len(sys.argv) > 2 else None

INIT = r"""
(() => {
  const sent = []; window.__sent = sent;
  const out = {id:'o1', name:'Feyonics Pad', send(b){ sent.push([...b]); }};
  Object.defineProperty(navigator,'requestMIDIAccess',{value: async()=>({outputs:new Map([['o1',out]]),inputs:new Map([['i1',(window.__inp={id:'i1',name:'Loop In',onmidimessage:null})]]),onstatechange:null}),configurable:true});
  const chr = new EventTarget(); chr.startNotifications = async()=>chr; chr.stopNotifications = async()=>chr;
  const srv = {getPrimaryService: async()=>({getCharacteristic: async()=>chr})};
  const dev = new EventTarget(); dev.name = 'TestPad';
  dev.gatt = {connected:false, connect: async()=>{ if(window.__btFail) throw new Error('radio off'); dev.gatt.connected=true; return srv; }, disconnect(){ dev.gatt.connected=false; }};
  Object.defineProperty(navigator,'bluetooth',{value:{requestDevice: async()=>dev},configurable:true});
  window.__emit = bytes => { chr.value = new DataView(Uint8Array.from(bytes).buffer); chr.dispatchEvent(new Event('characteristicvaluechanged')); };
  window.__drop = () => { dev.gatt.connected=false; dev.dispatchEvent(new Event('gattserverdisconnected')); };
  const ser = {requestPort: async()=>window.__nextPort, getPorts: async()=>[], addEventListener(){}, removeEventListener(){}};
  Object.defineProperty(navigator,'serial',{value:ser,configurable:true});
})();
"""
fails = []
def check(name, cond, extra=''):
    print(('PASS ' if cond else 'FAIL ') + name + ((' :: ' + str(extra)) if (extra and not cond) else ''))
    if not cond: fails.append(name)

with sync_playwright() as p:
    b = p.chromium.launch(executable_path='/usr/bin/google-chrome', args=['--no-sandbox'])
    ctx = b.new_context(viewport={'width':1400,'height':950}); ctx.add_init_script(INIT)
    pg = ctx.new_page(); errors = []
    pg.on('pageerror', lambda e: errors.append(str(e))); pg.on('console', lambda m: errors.append(m.text) if m.type=='error' else None)
    pg.goto(URL); pg.wait_for_timeout(400)

    # ---------- protocol helpers ----------
    r = pg.evaluate("""()=>({
      sk: __studio.cmdSetKey(0,0,1,{t:3,code:99,m1:128,m2:0,label:'Copy',text:''}),
      h1: __studio.parseHello('KM HELLO 1 12 4 4 1.24.2'), h2: __studio.parseHello('KM HELLO 2 4 4 4 2.0.0 596'),
      h3: __studio.parseHello('KM HELLO 9 1 1 1 1.0.0'), h4: __studio.parseHello('KM ERR x'),
      ble: __studio.bleParse(new DataView(Uint8Array.from([0x90,0x80,0x90,60,100,0x81,0x80,60,0]).buffer))})""")
    check('SETKEY string equals the Rust-tested one', r['sk']=='KM SETKEY 0 0 1 3 99 128 0 436F7079 -', r['sk'])
    check('HELLO macro', r['h1']['kind']=='macro' and r['h1']['fw']=='1.24.2')
    check('HELLO midi', r['h2']['kind']=='midi' and r['h2']['cfgSize']==596 and r['h2']['fw']=='2.0.0')
    check('HELLO unknown proto -> no kind', r['h3']['kind'] is None)
    check('HELLO error line -> null', r['h4'] is None)
    check('bleParse two messages', r['ble']==[[0x90,60,100],[0x80,60,0]], r['ble'])

    # ---------- bridge ----------
    check('MIDI output auto-picked', pg.eval_on_selector('#bOut','e=>e.selectedOptions[0].textContent')=='Feyonics Pad')
    pg.click('#bTest'); pg.wait_for_timeout(450)
    s = pg.evaluate('__sent'); check('test note on+off', s[:2]==[[0x90,60,100],[0x80,60,0]], s)
    pg.evaluate('__sent.length=0')
    pg.click('#bConnect'); pg.wait_for_timeout(300)
    check('bridge connected', pg.inner_text('#bPhase')=='Connected' and pg.inner_text('#bDev')=='TestPad')
    pg.evaluate("__emit([0x90,0x80,0x90,60,100])"); pg.evaluate("__emit([0x90,0x80,0x90,64,90])"); pg.evaluate("__emit([0x90,0x80,0xB0,64,127])")
    s = pg.evaluate('__sent'); check('notes forwarded', s==[[0x90,60,100],[0x90,64,90],[0xB0,64,127]], s)
    check('held counter', pg.inner_text('#sHeld')=='2')
    pg.evaluate('__sent.length=0'); pg.evaluate('__drop()'); pg.wait_for_timeout(150)
    s = pg.evaluate('__sent'); check('link drop releases notes + sustain', sorted(map(tuple,s))==sorted([(0x80,60,0),(0x80,64,0),(0xB0,64,0)]), s)
    check('phase reconnecting', pg.inner_text('#bPhase')=='Reconnecting')
    pg.wait_for_timeout(1400)
    check('auto reconnect', pg.inner_text('#bPhase')=='Connected' and pg.inner_text('#sRec')=='1', pg.inner_text('#bPhase'))
    pg.evaluate("window.__btFail=true"); pg.evaluate('__drop()'); pg.wait_for_timeout(1300)
    check('retry keeps trying while radio off', pg.inner_text('#bPhase')=='Reconnecting')
    pg.evaluate("window.__btFail=false"); pg.wait_for_timeout(2300)
    check('reconnect after failure', pg.inner_text('#bPhase')=='Connected', pg.inner_text('#bPhase'))
    pg.evaluate("__emit([0x90,0x80,0x90,72,100])"); pg.evaluate('__sent.length=0'); pg.click('#bConnect'); pg.wait_for_timeout(100)
    s = pg.evaluate('__sent'); check('manual disconnect releases held note', [0x80,72,0] in s, s)
    check('idle after disconnect', pg.inner_text('#bPhase')=='Idle')

    # ---------- macro pad ----------
    pg.click('.nav[data-p=pads]'); pg.click('#pDemoMacro'); pg.wait_for_selector('.kgrid', timeout=8000)
    check('macro detected', 'macro' in pg.inner_text('#navP') and 'v1.24.2' in pg.inner_text('#phFw'))
    st = pg.evaluate("({n:__studio.M.layers.length,names:__studio.M.layers.map(l=>l.name),en:__studio.M.enable,k1:__studio.M.layers[0].keys[1],txt:__studio.M.layers[0].keys[3],pn:__studio.M.padname,set:__studio.M.settings,bt:__studio.M.bt})")
    check('12 layers read', st['n']==12 and st['names'][1]=='Claude app' and st['names'][11]=='Excel')
    check('enable flags', st['en'][:5]==[True]*5 and st['en'][5]==False)
    check('key decoded', st['k1']['t']==3 and st['k1']['code']==99 and st['k1']['m1']==128 and st['k1']['label']=='Copy')
    check('text macro decoded', st['txt']['text']=='hello@example.com')
    check('pad name / settings / bt', st['pn']=={'owner':'Demo','device':'Pad'} and st['set']['bright']==80 and st['bt']['bonds']==1 and st['bt']['name']=='Demo', st)
    check('no unsaved at start', pg.evaluate('__studio.macroDiff().length')==0)
    if SHOT: pg.screenshot(path=SHOT.replace('.png','-keys.png'))
    # edit key 4 -> Combo Ctrl+Z
    pg.click('[data-key="4"]'); pg.select_option('[data-f=type]','3'); pg.select_option('#insp [data-f=m1]','128'); pg.select_option('#insp [data-f=code]', str(ord('z')))
    pg.fill('#insp [data-f=label]','Undo')
    check('combo tile text', 'Undo' in pg.inner_text('[data-key="4"]'))
    check('one unsaved change', '1 unsaved change' in pg.inner_text('#mBar'), pg.inner_text('#mBar'))
    # TRANS not offered for knobs
    pg.click('[data-enc="2"]'); opts = pg.eval_on_selector_all('#insp [data-f=type] option','o=>o.map(x=>x.textContent)')
    check('knob cannot be Same-as-layer-1', 'Same as layer 1' not in opts and 'Scroll' in opts, opts)
    pg.select_option('[data-f=type]','6'); pg.select_option('#insp [data-f=code]','2'); pg.click('[data-key="0"]')
    pg.click('[data-a=save]'); pg.wait_for_timeout(700)
    log = pg.evaluate("document.querySelector('#mBar').textContent")
    sim = pg.evaluate("__studio.pad.port.sim.log")
    check('sent SETKEY for key 4', any(l.startswith('SETKEY 0 1 0 3 122 128 0 ') for l in sim), sim)
    check('sent SETENC for knob', any(l.startswith('SETENC 0 2 6 2 0 0') for l in sim), sim)
    check('SAVE sent after changes', sim and sim[-1]=='SAVE', sim)
    check('saved state clean', pg.evaluate('__studio.macroDiff().length')==0 and 'Saved on the pad' in log, log)
    check('only changed things were sent', len([l for l in sim if l.startswith('SET')])==2, sim)
    # too-big keymap
    pg.evaluate("__studio.pad.port.sim.cap=10"); pg.click('[data-key="6"]'); pg.select_option('[data-f=type]','4'); pg.fill('#insp [data-f=text]','x'*51)
    pg.click('[data-a=save]'); pg.wait_for_timeout(700)
    check('keymap too big surfaces the pad error', 'keymap too big' in pg.inner_text('#mBar'), pg.inner_text('#mBar'))
    check('still unsaved after failure', pg.evaluate('__studio.macroDiff().length')>0)
    pg.evaluate("__studio.pad.port.sim.cap=4000"); pg.click('[data-a=save]'); pg.wait_for_timeout(700)
    check('saves after the limit is fixed', pg.evaluate('__studio.macroDiff().length')==0)
    # layers tab
    pg.click('[data-tab=layers]'); pg.fill('[data-ln="1"]','Claude!'); pg.check('[data-en="6"]')
    pg.select_option('[data-lt="2"]','2'); pg.fill('[data-lnm="2"]','Terminal')
    pg.click('[data-a=save]'); pg.wait_for_timeout(900)
    sim = pg.evaluate("__studio.pad.port.sim"); 
    check('layer renamed on pad', sim['layers'][1]['name']=='Claude!')
    check('layer enabled on pad', sim['enable'][6]==True)
    check('launch action on pad', sim['launch'][2][0]==2 and sim['launch'][2][4]=='Terminal', sim['launch'][2])
    if SHOT: pg.screenshot(path=SHOT.replace('.png','-layers.png'))
    # display tab: immediate commands
    pg.click('[data-tab=display]'); pg.select_option('[data-sel=theme]','13'); pg.wait_for_timeout(200)
    pg.fill('[data-set=bright]','35'); pg.dispatch_event('[data-set=bright]','input'); pg.wait_for_timeout(700)
    pg.check('[data-chk=hideName]'); pg.wait_for_timeout(200)
    sim = pg.evaluate("__studio.pad.port.sim.set")
    check('theme/brightness/hide-name sent', sim[0]==13 and sim[2]==35 and sim[12]==1, sim)
    if SHOT: pg.screenshot(path=SHOT.replace('.png','-display.png'))
    pg.click('[data-tab=bt]'); pg.select_option('#btMode','1'); pg.click('[data-a=pair]'); pg.wait_for_timeout(300)
    sim = pg.evaluate("__studio.pad.port.sim"); check('bluetooth mode + pairing sent', sim['set'][9]==1 and sim['bt'][2]==2, sim['bt'])
    pg.click('[data-tab=pad]'); pg.fill('#pnOwner','Sam'); pg.click('[data-a=save]'); pg.wait_for_timeout(600)
    check('pad name saved', pg.evaluate("__studio.pad.port.sim.pad[0]")=='Sam')
    # dirty edits survive tab switches
    pg.click('[data-tab=keys]'); pg.click('[data-key="9"]'); pg.select_option('[data-f=type]','2'); pg.click('[data-tab=display]'); pg.click('[data-tab=keys]')
    check('unsaved edit survives tab switch', pg.evaluate('__studio.macroDiff().length')==1)
    pg.once('dialog', lambda d: d.accept()); pg.click('[data-a=reload]'); pg.wait_for_timeout(800)
    check('reload discards edits', pg.evaluate('__studio.macroDiff().length')==0)
    pg.click('#phDisc'); pg.wait_for_timeout(200)
    check('disconnect returns to start', pg.is_visible('#padHome') and pg.evaluate('__studio.pad.port')is None)

    # ---------- MIDI pad: detection + editor hosting ----------
    port = pg.evaluate("(window.__nextPort=__studio.makeSim('midi'),1)"); pg.click('#pConnect'); pg.wait_for_selector('#mframe:not([hidden])', timeout=8000); pg.wait_for_timeout(1200)
    check('midi detected', 'MIDI' in pg.inner_text('#navP') and 'v2.0.0' in pg.inner_text('#phFw'))
    fr = [f for f in pg.frames if f != pg.main_frame][0]
    info = fr.evaluate("({h:window.hello,z:window.zdata,t:document.getElementById('st').textContent})")
    check('editor got the shell\'s port and talked to the pad', info['h'].startswith('KM HELLO 2') and info['z']>1000, info)
    pg.click('#phDisc'); pg.wait_for_timeout(500)
    check('port released after leaving the editor', pg.evaluate("window.__feyonicsPort._open")==False)
    # reconnect to the same sim port works (port reusable after release)
    pg.evaluate("window.__nextPort=window.__feyonicsPort"); pg.click('#pConnect'); pg.wait_for_selector('#mframe:not([hidden])', timeout=8000); pg.wait_for_timeout(800)
    fr = [f for f in pg.frames if f != pg.main_frame][0]; check('second open of the MIDI editor works', fr.evaluate('window.zdata')>1000)
    if SHOT: pg.screenshot(path=SHOT.replace('.png','-midi.png'))
    pg.click('#phDisc'); pg.wait_for_timeout(300)

    # ---------- Serial MIDI ----------
    got = pg.evaluate("""(()=>{const o=[];const f=__studio.MidiParser(m=>o.push(m));[0x90,60,100,62,100,0xF8,0x80,60,0,0xC0,5,0xF0,1,2,0xF7,0xB0,64,127,0xF1,0x10,0x45,7,0x91,70,80].forEach(f);return o})()""")
    check('serial MIDI parser: running status, realtime, program, sysex, MTC', got==[[0x90,60,100],[0x90,62,100],[0xF8],[0x80,60,0],[0xC0,5],[0xF0,1,2,0xF7],[0xB0,64,127],[0xF1,0x10],[0x91,70,80]], got)
    pg.click('.nav[data-p=serial]')
    pg.evaluate("""(()=>{let ctrl;const writes=[];window.__rawW=writes;const mk=()=>({_o:false,readable:null,writable:null,async open(o){this._o=true;this.baud=o.baudRate;window.__baud=o.baudRate;this.readable=new ReadableStream({start(c){ctrl=c}});this.writable=new WritableStream({write(ch){writes.push([...ch])}})},async close(){this._o=false;this.readable=this.writable=null},getInfo(){return{}}});
      window.__mkRaw=()=>{window.__nextPort=mk()};window.__rawPush=b=>ctrl.enqueue(Uint8Array.from(b));window.__rawClose=()=>ctrl.close();window.__mkRaw()})()""")
    pg.select_option('#smOut','o1'); pg.select_option('#smBaud','31250'); pg.evaluate('__sent.length=0'); pg.click('#smConnect'); pg.wait_for_timeout(300)
    check('serial opened at chosen baud', pg.evaluate('window.__baud')==31250 and pg.inner_text('#smPhase')=='Open')
    pg.evaluate('__rawPush([0x90,60,100,0x90,64,90])'); pg.wait_for_timeout(200)
    s = pg.evaluate('__sent'); check('serial bytes -> MIDI out', s==[[0x90,60,100],[0x90,64,90]], s)
    pg.select_option('#smMidiIn','i1'); pg.evaluate('__inp.onmidimessage({data:[0x90,40,90]})'); pg.wait_for_timeout(100)
    w = pg.evaluate('__rawW'); check('MIDI in -> serial', w==[[0x90,40,90]], w)
    pg.evaluate('__sent.length=0'); pg.click('#smConnect'); pg.wait_for_timeout(300)
    s = pg.evaluate('__sent'); check('closing releases held notes', sorted(map(tuple,s))==[(0x80,60,0),(0x80,64,0)] and pg.inner_text('#smPhase')=='Closed', s)
    pg.evaluate('__mkRaw()'); pg.click('#smConnect'); pg.wait_for_timeout(300); pg.evaluate('__rawPush([0x92,50,100])'); pg.wait_for_timeout(100); pg.evaluate('__sent.length=0'); pg.evaluate('__rawClose()'); pg.wait_for_timeout(300)
    s = pg.evaluate('__sent'); check('device unplugged: notes released, port shown closed', [0x82,50,0] in s and pg.inner_text('#smPhase')=='Closed', s)

    # ---------- unknown device ----------
    pg.evaluate("""(()=>{const p=__studio.makeSim('macro');const o=p.open.bind(p);p.open=async()=>{await o();};
       const h=p.writable;window.__nextPort=p; p.sim.noHello=1})()""")
    errors_f=[e for e in errors if '404' not in e]
    check('no page errors', not errors_f, errors_f)
    b.close()
print('\nFAILED:', fails if fails else 'none'); sys.exit(1 if fails else 0)
