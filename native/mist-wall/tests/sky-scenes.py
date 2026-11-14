#!/usr/bin/env python3
# The weather sky for every weather case it has a scene for, offscreen: one PNG per case and a contact sheet
# (sheet.png), to look at after touching shaders/sky.frag, shaders/clouds.frag or native/common/sky.rs.
#   native/mist-wall/tests/sky-scenes.py <out dir> [case ...]
#   native/mist-wall/tests/sky-scenes.py <out dir> lands     every landscape through one day (lands.png): the Sun and
#                                                          the Moon cross the sky, the land changes with the hour
# MIST_SKY_LAND=0..4 (fields, lake, mountains, city, sea) picks the landscape for the cases.
# Each case is a weather.json reading and a moment (MIST_SKY_NOW); storms are drawn mid-strike (MIST_SKY_T).
# Needs the release build of mist-wall, a GPU and ImageMagick.
import json, os, subprocess, sys, datetime, zoneinfo
REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
out = os.path.abspath(sys.argv[1]); only = sys.argv[2:]
os.makedirs(out, exist_ok=True)
tz = zoneinfo.ZoneInfo('Europe/Warsaw')
def ms(mo, d, h, mi=0): return int(datetime.datetime(2026, mo, d, h, mi, tzinfo=tz).timestamp() * 1000)
B = dict(lat=52.2297, lon=21.0122, code=0, temp=8, cloud=5, precip=0, snow=0, wind=8, gust=12, vis=30000, hum=60, windDir=266, snowDepth=0, kp=1)
def sc(name, when, t=100.0, **k): return (name, when, t, {**B, **k})
SC = [
 sc('clear-noon', ms(7,15,13), temp=24),
 sc('golden-hour', ms(11,13,15,25), code=1, cloud=25),
 sc('dawn', ms(11,13,6,50), code=1, cloud=20, temp=2, hum=88),
 sc('night-moon', ms(11,22,21), code=0, cloud=8, temp=1),
 sc('aurora', ms(11,13,23), kp=7.5, temp=-2),
 sc('partly', ms(11,13,11), code=2, cloud=45),
 sc('overcast', ms(11,13,11), code=3, cloud=96),
 sc('drizzle', ms(11,13,11), code=53, cloud=92, precip=0.08, hum=93),
 sc('rain', ms(11,13,11), code=63, cloud=95, precip=0.6, wind=18, gust=30),
 sc('downpour', ms(7,15,15), code=82, cloud=98, precip=3.5, wind=24, gust=45, temp=17),
 sc('storm', ms(7,15,19), t=101.0, code=95, cloud=98, precip=2.0, wind=30, gust=60, temp=19),
 sc('hail', ms(7,15,16), t=101.0, code=99, cloud=98, precip=2.5, wind=28, gust=55, temp=15),
 sc('rainbow', ms(7,15,18,30), code=80, cloud=55, precip=0.3, temp=18),
 sc('snow', ms(1,13,11), code=73, cloud=95, snow=0.3, precip=0.3, temp=-3, snowDepth=0.06),
 sc('blizzard', ms(1,13,13), code=86, cloud=100, snow=1.2, precip=1.2, temp=-6, wind=45, gust=75, snowDepth=0.25, vis=600),
 sc('sleet', ms(1,13,11), code=61, cloud=95, precip=0.4, temp=1),
 sc('freezing-rain', ms(1,13,11), code=66, cloud=96, precip=0.4, temp=-1),
 sc('fog', ms(11,13,9), code=45, cloud=100, vis=300, hum=98, temp=4),
 sc('rime-fog', ms(1,13,9), code=48, cloud=100, vis=300, hum=98, temp=-4, snowDepth=0.02),
 sc('haze', ms(7,15,12), code=1, cloud=20, vis=2500, temp=27),
 sc('gale', ms(11,13,12), code=2, cloud=60, wind=55, gust=92, temp=9),
 sc('heat', ms(7,15,14), code=0, cloud=5, temp=34, hum=35),
 sc('frost-clear', ms(1,13,12), code=0, cloud=5, temp=-12, snowDepth=0.12),
 sc('snow-night', ms(1,13,21), code=71, cloud=90, snow=0.15, precip=0.15, temp=-4, snowDepth=0.08),
 sc('storm-night', ms(7,15,23), t=101.0, code=95, cloud=98, precip=2.0, wind=30, gust=60, temp=19),
 sc('summer-night', ms(7,15,23,30), code=0, cloud=5, temp=18),
]
import math
def h1(a, b):
    x = math.sin(a * 127.1 + b * 311.7) * 43758.5453
    return x - math.floor(x)
def strike():
    for slot in range(10, 400):
        if h1(slot, 9.1) < 0.55: return slot * 3.2 + h1(slot, 8.3) * 2.2 + 0.03
BOLT = strike()
SC = [(n, w, BOLT if t == 101.0 else t, x) for n, w, t, x in SC]
hr = lambda when, pr: [dict(t=when // 1000 - 3600 * i, pr=pr) for i in range(1, 4)]
SC += [
 sc('after-rain', ms(7,15,17), code=2, cloud=45, temp=19, hum=85, hourly=hr(ms(7,15,17), 0.8)),
 sc('ground-fog', ms(9,20,6,50), code=1, cloud=15, temp=7, hum=97, wind=3, gust=5),
 sc('ground-fog-night', ms(9,20,23), code=0, cloud=10, temp=9, hum=96, wind=4, gust=6),
 sc('diamond-dust', ms(1,13,13), code=0, cloud=8, temp=-16, snowDepth=0.15, hum=70),
 sc('overcast-sun', ms(11,13,12), code=3, cloud=100),
]
W, H = 546, 310
LANDS = ['fields', 'lake', 'mountains', 'city', 'sea']
DAY = [('05:30', ms(7,15,5,30), {}), ('08:00', ms(7,15,8), {}), ('13:00', ms(7,15,13), {}), ('18:00', ms(7,15,18), {}), ('20:40', ms(7,15,20,40), {}),
       ('00:30', ms(7,30,0,30), {}), ('snow', ms(1,13,13), dict(code=71, cloud=70, snow=0.1, precip=0.1, temp=-4, snowDepth=0.1))]
lands = only == ['lands']
if lands:
    SC = [sc(f'{l}-{n}', when, **{**dict(land=i, code=1, cloud=18, temp=20), **k}) for i, l in enumerate(LANDS) for n, when, k in DAY]
    only = []
for name, when, t, wx in SC:
    if only and name not in only: continue
    home = f'{out}/home'; os.makedirs(f'{home}/.cache/mist', exist_ok=True)
    json.dump(wx, open(f'{home}/.cache/mist/weather.json', 'w'))
    env = dict(os.environ, HOME=home, MIST_DIR=REPO, MIST_SKY_NOW=str(when), MIST_SKY_T=str(t))
    if 'land' in wx: env['MIST_SKY_LAND'] = str(wx['land'])
    r = subprocess.run([f'{REPO}/native/target/release/mist-wall', '--png', f'{out}/{name}.ppm', 'sky', str(W), str(H)], env=env, capture_output=True, text=True)
    if not os.path.exists(f'{out}/{name}.ppm'): print(name, 'FAILED', r.stderr[-400:]); continue
    subprocess.run(['magick', '-size', f'{W}x{H}', 'xc:#1b2430', f'{out}/{name}.ppm', '-composite', '-gravity', 'southwest', '-fill', 'white', '-pointsize', '13', '-annotate', '+8+6', name, f'{out}/{name}.png'])
names = [n for n, *_ in SC if not only or n in only]
subprocess.run(['magick', 'montage'] + [f'{out}/{n}.png' for n in names] + ['-tile', f'{len(DAY)}x' if lands else '4x', '-geometry', '+4+4', '-background', '#10151c', f'{out}/lands.png' if lands else f'{out}/sheet.png'])
