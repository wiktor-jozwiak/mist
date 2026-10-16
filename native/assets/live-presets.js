.pragma library
// Live wallpapers (components/LiveWall.qml, shaders/live.frag). Dark, low-contrast glow scenes tuned to
// the desktop widgets: white text stays readable, the teal accent (#8ad6d0) fits every palette.
// mode: 0 glow, 1 aurora, 2 nebula, 3 silk, 4 lava, 5 horizon, 6 bokeh, 7 caustics, 8 rain on glass, 9 topo,
// 10 synthwave, 11 stars, 12 dunes, 13 mesh, 14 mountains, 15 light waves, 16 snow, 17 fireflies, 18 dots,
// 19 cells, 20 smoke, 21 pond. shader: "toon" -> shaders/toon.frag (its own modes 0..15), "fx" -> shaders/fx.frag (0..21), res = render scale. weather: the scene itself follows the weather (LiveWall.weatherMode).
// stars / vig / bright -> shader `opt`. auto: palette follows the real sun (and the weather), see day().
// sun (horizon without auto): [x, y, disc size, strength], strength < 0 = moon.
var list = [
    // ---- follow the sun and the weather
    { id: "day", desc: "Kolory nieba o tej porze: noc, niebieska godzina, złota godzina, dzień. Szarzeje, gdy pada.", cat: "auto", name: "Pora dnia", mode: 0, auto: true, c: ["#06121e", "#123e6a", "#2a78a6", "#9ad8e0"], stars: 0, vig: 0.85, bright: 0.85, speed: 1 },
    { id: "horizon", desc: "Morze i niebo. Słońce albo księżyc stoją tam, gdzie naprawdę są.", cat: "auto", name: "Horyzont", mode: 5, auto: true, c: ["#06121e", "#123e6a", "#2a78a6", "#9ad8e0"], stars: 0, vig: 0.7, bright: 0.85, speed: 1 },
    { id: "peaks", desc: "Warstwy gór, mgła w dolinach, prawdziwe słońce i księżyc.", cat: "auto", name: "Góry", mode: 14, auto: true, c: ["#06121e", "#123e6a", "#2a78a6", "#9ad8e0"], stars: 0, vig: 0.7, bright: 0.9, speed: 1 },
    { id: "window", desc: "To, co za oknem: deszcz na szybie, śnieg, mgła, gwiazdy albo góry.", cat: "auto", name: "Za oknem", mode: 14, auto: true, weather: true, c: ["#06121e", "#123e6a", "#2a78a6", "#9ad8e0"], stars: 0, vig: 0.75, bright: 0.9, speed: 1 },
    // ---- cartoons (shaders/toon.frag, full resolution)
    { id: "cat", desc: "Czarny kot na kalenicy, księżyc, dym z komina i światła miasta.", cat: "toon", shader: "toon", res: 1, name: "Kot na dachu", mode: 0, c: ["#070b1e", "#2c2458", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "whales", desc: "Wieloryby płyną powoli przez smugi światła w głębinie.", cat: "toon", shader: "toon", res: 1, name: "Wieloryby", mode: 1, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "jelly", desc: "Świecące meduzy unoszą się w rytmie oddechu.", cat: "toon", shader: "toon", res: 1, name: "Meduzy", mode: 2, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "koi", desc: "Karpie krążą w stawie pod liśćmi lilii.", cat: "toon", shader: "toon", res: 1, name: "Karpie koi", mode: 3, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "astro", desc: "Mały astronauta macha zza planety z pierścieniem.", cat: "toon", shader: "toon", res: 1, name: "Astronauta", mode: 4, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "lighthouse", desc: "Latarnia morska omiata noc snopem światła.", cat: "toon", shader: "toon", res: 1, name: "Latarnia", mode: 5, c: ["#050a1c", "#1c2448", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "fox", desc: "Lis siedzi na śniegu i patrzy na zorzę.", cat: "toon", shader: "toon", res: 1, name: "Lis i zorza", mode: 6, c: ["#04101a", "#123040", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "balloons", desc: "Balony dryfują nad morzem chmur o zachodzie.", cat: "toon", shader: "toon", res: 1, name: "Balony", mode: 7, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "ghosts", desc: "Halloween: duszki, dynie z mrugającymi buziami i nietoperze.", cat: "toon", shader: "toon", res: 1, name: "Duszki", mode: 8, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "penguins", desc: "Pingwiny na krze, jeden ciągle gdzieś drepcze.", cat: "toon", shader: "toon", res: 1, name: "Pingwiny", mode: 9, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "rocket", desc: "Rakieta pędzi w górę między planetami.", cat: "toon", shader: "toon", res: 1, name: "Rakieta", mode: 10, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "owl", desc: "Sowa na gałęzi przed księżycem, rozgląda się i mruga.", cat: "toon", shader: "toon", res: 1, name: "Sowa", mode: 11, c: ["#050a14", "#16223c", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "pixel", desc: "Pikselowe miasto nocą i przejeżdżający pociąg.", cat: "toon", shader: "toon", res: 1, name: "Miasto 8-bit", mode: 12, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "boats", desc: "Papierowe łódki i lampiony unoszące się nad wodą.", cat: "toon", shader: "toon", res: 1, name: "Lampiony", mode: 13, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "aquarium", desc: "Ławica kolorowych rybek, wodorosty i bąbelki.", cat: "toon", shader: "toon", res: 1, name: "Akwarium", mode: 14, c: ["#000000", "#000000", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    { id: "bunny", desc: "Królik na chmurce łowi gwiazdkę na wędkę.", cat: "toon", shader: "toon", res: 1, name: "Królik i gwiazdka", mode: 15, c: ["#070b22", "#2a2260", "#3a5a8a", "#8ad6d0"], stars: 0, vig: 0.55, bright: 0.92, speed: 1 },
    // ---- calm
    { id: "mesh", desc: "Miękkie plamy koloru, które powoli się mieszają.", cat: "calm", name: "Gradient", mode: 13, c: ["#0d3a4a", "#1f5f7a", "#3a6a8a", "#5a4a8a"], stars: 0, vig: 0.8, bright: 0.72, speed: 0.8 },
    { id: "dusk", desc: "Róż i bursztyn zachodu, płynnie przechodzące w granat.", cat: "calm", name: "Zmierzch", mode: 13, c: ["#2a1a3a", "#6a2a4a", "#1a2a5a", "#a8603a"], stars: 0, vig: 0.8, bright: 0.68, speed: 0.8 },
    { id: "lavender", desc: "Fiolet i błękit, bardzo spokojny gradient.", cat: "calm", name: "Lawenda", mode: 13, c: ["#2a2a5a", "#4a3a7a", "#1a4a6a", "#7a5a9a"], stars: 0, vig: 0.8, bright: 0.7, speed: 0.8 },
    { id: "aurora", desc: "Wstęga zorzy z falującymi kurtynami światła.", cat: "calm", name: "Zorza", mode: 1, c: ["#060a12", "#1c8c7c", "#2a5aa8", "#34266a"], stars: 0, vig: 0.8, bright: 0.95, speed: 1 },
    { id: "deepsea", desc: "Turkusowe światło płynące przez ciemną wodę.", cat: "calm", name: "Głębia", mode: 0, c: ["#040a10", "#0c3444", "#1a7078", "#5fb3ad"], stars: 0, vig: 0.9, bright: 0.85, speed: 0.8 },
    { id: "nebula", desc: "Fioletowa mgławica i migające gwiazdy.", cat: "calm", name: "Mgławica", mode: 2, c: ["#06050d", "#24134a", "#4a2470", "#1c467e"], stars: 0.9, vig: 0.9, bright: 0.9, speed: 0.7 },
    { id: "silk", desc: "Fałdy światła jak przesuwająca się tkanina.", cat: "calm", name: "Jedwab", mode: 3, c: ["#070a10", "#122c3e", "#225862", "#3c7a78"], stars: 0, vig: 0.85, bright: 0.8, speed: 1 },
    { id: "topo", desc: "Poziomice mapy, które powoli płyną i zmieniają kształt.", cat: "calm", name: "Topografia", mode: 9, c: ["#05090f", "#0b1a26", "#3a8a8a", "#8ad6d0"], stars: 0, vig: 0.85, bright: 0.85, speed: 0.8 },
    { id: "map", desc: "Poziomice w ciepłym bursztynie, jak stara mapa.", cat: "calm", name: "Mapa", mode: 9, c: ["#0a0706", "#1a120c", "#8a5a2a", "#ecc48e"], stars: 0, vig: 0.85, bright: 0.8, speed: 0.8 },
    { id: "cells", desc: "Kryształowe komórki ze świecącymi krawędziami.", cat: "calm", name: "Kryształy", mode: 19, c: ["#05070c", "#0f1a2a", "#2a5a7a", "#8ad6d0"], stars: 0, vig: 0.85, bright: 0.85, speed: 0.8 },
    { id: "dots", desc: "Siatka kropek, przez którą przechodzi fala światła.", cat: "calm", name: "Kropki", mode: 18, c: ["#05070b", "#0e1622", "#3a8aa0", "#8ad6d0"], stars: 0, vig: 0.8, bright: 0.9, speed: 1 },
    { id: "smoke", desc: "Smugi dymu unoszące się w ciemności.", cat: "calm", name: "Dym", mode: 20, c: ["#06060a", "#1a1a2a", "#3a3a5a", "#7a8aa0"], stars: 0, vig: 0.85, bright: 0.85, speed: 0.8 },
    { id: "pond", desc: "Krople spadają na taflę i rozchodzą się kręgami.", cat: "calm", name: "Staw", mode: 21, c: ["#040a10", "#0c2030", "#2a6a7a", "#8ad6d0"], stars: 0, vig: 0.85, bright: 0.9, speed: 1 },
    { id: "dunes", desc: "Wydmy o zmierzchu, piasek wiejący z grzbietów.", cat: "calm", name: "Wydmy", mode: 12, c: ["#0a0608", "#241628", "#7a4226", "#d8985a"], stars: 0.5, vig: 0.8, bright: 0.85, speed: 1 },
    { id: "snow", desc: "Padający śnieg w kilku warstwach głębi.", cat: "calm", name: "Śnieg", mode: 16, c: ["#03060c", "#0e1a2e", "#2a3a5a", "#6a8ab0"], stars: 0, vig: 0.8, bright: 0.85, speed: 1 },
    { id: "ember", desc: "Ciemny, ciepły żar.", cat: "calm", name: "Żar", mode: 0, c: ["#0a0606", "#34110b", "#7a3412", "#b0642a"], stars: 0, vig: 0.95, bright: 0.8, speed: 0.8 },
    { id: "abyss", desc: "Prawie czarna, ledwo się porusza.", cat: "calm", name: "Otchłań", mode: 0, c: ["#030509", "#0a1322", "#112c46", "#1b4a58"], stars: 0.25, vig: 0.9, bright: 0.8, speed: 0.6 },
    // ---- lively
    // ---- lively, shaders/fx.frag
    { id: "warp", desc: "Gwiazdy rozciągają się w smugi, lecisz w nadświetlnej.", cat: "live", shader: "fx", res: 0.5, name: "Hiperprzestrzeń", mode: 0, c: ["#03040a", "#2a3a8a", "#8ad6ff", "#ffffff"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "warp2", desc: "Skok w nadprzestrzeń w fiolecie i różu.", cat: "live", shader: "fx", res: 0.5, name: "Warp fiolet", mode: 0, c: ["#06030c", "#5a2a9a", "#f05aa8", "#b48ad6"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "tunnel", desc: "Pędzisz przez tunel z neonowych pierścieni.", cat: "live", shader: "fx", res: 0.5, name: "Neonowy tunel", mode: 1, c: ["#04030a", "#3a1060", "#3ac0e0", "#f05aa8"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "tunnel2", desc: "Ciepły, złoty tunel bez końca.", cat: "live", shader: "fx", res: 0.5, name: "Złoty tunel", mode: 1, c: ["#060402", "#4a2a08", "#f0a050", "#ffe0a0"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "code", desc: "Spadające kolumny znaków jak w terminalu hakera.", cat: "live", shader: "fx", res: 1, name: "Kod", mode: 2, c: ["#010402", "#0a3a1a", "#2ad06a", "#c8ffd8"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "code2", desc: "Deszcz znaków w turkusie pod kolor widżetów.", cat: "live", shader: "fx", res: 1, name: "Kod turkus", mode: 2, c: ["#01040a", "#0a2a3a", "#3ac0d0", "#d8ffff"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "plasma", desc: "Płynąca, kolorowa plazma.", cat: "live", shader: "fx", res: 0.5, name: "Plazma", mode: 3, c: ["#05030c", "#3a1a8a", "#d03a8a", "#3ac0e0"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "storm", desc: "Kłębiące się chmury, deszcz i rzadkie, łagodne błyski.", cat: "live", shader: "fx", res: 0.5, name: "Burza", mode: 4, c: ["#04060c", "#1a2236", "#2a3450", "#b8c8ff"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "fire", desc: "Trzaskające ognisko w lesie, iskry lecą w górę.", cat: "live", shader: "fx", res: 0.5, name: "Ognisko", mode: 5, c: ["#04030a", "#141026", "#1a1a1a", "#ff8a3a"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "fireworks", desc: "Wybuchające fajerwerki nad miastem.", cat: "live", shader: "fx", res: 0.5, name: "Fajerwerki", mode: 6, c: ["#03040c", "#1a1a40", "#f05aa8", "#ffd060"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "fireworks2", desc: "Złote i błękitne fajerwerki.", cat: "live", shader: "fx", res: 0.5, name: "Sylwester", mode: 6, c: ["#02030a", "#0a1a3a", "#8ad6ff", "#ffe08a"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "galaxy", desc: "Obracająca się galaktyka spiralna.", cat: "live", shader: "fx", res: 0.5, name: "Galaktyka", mode: 7, c: ["#020208", "#3a2a8a", "#3ac0e0", "#ffd8a8"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "hole", desc: "Wirujący dysk wokół czarnej dziury, zakrzywione światło gwiazd.", cat: "live", shader: "fx", res: 0.5, name: "Czarna dziura", mode: 8, c: ["#010104", "#1a1030", "#ff8a3a", "#fff0c0"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "ocean", desc: "Toczące się fale z pianą pod księżycem.", cat: "live", shader: "fx", res: 0.5, name: "Fale oceanu", mode: 9, c: ["#02060e", "#1a3a6a", "#1a6a7a", "#ffd8c0"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "kaleido", desc: "Hipnotyczny, obracający się kalejdoskop.", cat: "live", shader: "fx", res: 0.5, name: "Kalejdoskop", mode: 10, c: ["#03030a", "#3a1a8a", "#3ac0e0", "#f05aa8"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "plexus", desc: "Punkty dryfują i łączą się liniami.", cat: "live", shader: "fx", res: 0.75, name: "Sieć", mode: 11, c: ["#03050c", "#1a3a6a", "#3ac0e0", "#8ad6d0"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "plexus2", desc: "Sieć świecących punktów w lawendzie.", cat: "live", shader: "fx", res: 0.75, name: "Konstelacje", mode: 11, c: ["#05030c", "#2a1a5a", "#b48ad6", "#ffd8f0"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "eq", desc: "Słupki equalizera, z muzyką rosną.", cat: "live", shader: "fx", res: 0.5, name: "Equalizer", mode: 12, c: ["#03040a", "#1a3a8a", "#3ac0e0", "#f05aa8"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "eq2", desc: "Equalizer w barwach starego wzmacniacza.", cat: "live", shader: "fx", res: 0.5, name: "Equalizer retro", mode: 12, c: ["#050305", "#3a1a0a", "#ffb03a", "#ff4a3a"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "orbits", desc: "Planety krążą po orbitach wokół słońca.", cat: "live", shader: "fx", res: 0.5, name: "Układ planet", mode: 13, c: ["#020308", "#2a3a6a", "#3ac0e0", "#ffd080"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "bubbles", desc: "Mieniące się bańki mydlane unoszą się w górę.", cat: "live", shader: "fx", res: 0.5, name: "Bańki", mode: 14, c: ["#040810", "#1a3a5a", "#3ac0e0", "#ffffff"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "meteors", desc: "Spadające gwiazdy co chwilę przecinają niebo.", cat: "live", shader: "fx", res: 0.75, name: "Deszcz meteorów", mode: 15, c: ["#020410", "#1a2a5a", "#3a5aa8", "#c8e0ff"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "meadow", desc: "Trawa faluje na wietrze o zachodzie, lecą ptaki.", cat: "live", shader: "fx", res: 0.5, name: "Łąka", mode: 16, c: ["#0a0610", "#3a3a8a", "#3a6a3a", "#ff9a6a"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "highway", desc: "Smugi świateł aut na nocnej autostradzie.", cat: "live", shader: "fx", res: 0.75, name: "Autostrada", mode: 17, c: ["#03030a", "#2a1a4a", "#3ac0e0", "#ff8a6a"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "vortex", desc: "Kolory wciągane w wirujący lej.", cat: "live", shader: "fx", res: 0.5, name: "Wir", mode: 18, c: ["#030308", "#3a1a8a", "#f05aa8", "#3ac0e0"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "ribbons", desc: "Skręcające się neonowe wstęgi.", cat: "live", shader: "fx", res: 0.75, name: "Neonowe wstęgi", mode: 19, c: ["#03030a", "#3ac0e0", "#f05aa8", "#b48ad6"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "ribbons2", desc: "Wstęgi w złocie i pomarańczu.", cat: "live", shader: "fx", res: 0.75, name: "Złote wstęgi", mode: 19, c: ["#050302", "#ffb03a", "#ff6a3a", "#fff0b0"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "globe", desc: "Obracający się holograficzny glob.", cat: "live", shader: "fx", res: 0.75, name: "Hologram", mode: 20, c: ["#020610", "#0a2a4a", "#3ac0e0", "#8affff"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "dna", desc: "Podwójna helisa obraca się w przestrzeni.", cat: "live", shader: "fx", res: 0.75, name: "DNA", mode: 21, c: ["#03040c", "#1a2a5a", "#3ac0e0", "#f05aa8"], stars: 0, vig: 0.6, bright: 0.92, speed: 1 },
    { id: "rain", desc: "Krople spływają po szybie, za nią rozmyte światła miasta.", cat: "live", name: "Deszcz na szybie", mode: 8, c: ["#04060c", "#1a2a44", "#e0a050", "#8ad6d0"], stars: 0, vig: 0.8, bright: 0.9, speed: 1 },
    { id: "neonrain", desc: "Deszcz na szybie i neony w tle.", cat: "live", name: "Neonowy deszcz", mode: 8, c: ["#06040c", "#2a1040", "#f05aa8", "#3ac0e0"], stars: 0, vig: 0.8, bright: 0.85, speed: 1 },
    { id: "stars", desc: "Dryfujące gwiazdy, co jakiś czas spadająca gwiazda.", cat: "live", name: "Gwiazdy", mode: 11, c: ["#02040a", "#0a1430", "#24306a", "#6a5aa0"], stars: 0, vig: 0.8, bright: 0.9, speed: 1 },
    { id: "fireflies", desc: "Świetliki nad łąką pod nocnym niebem.", cat: "live", name: "Świetliki", mode: 17, c: ["#020406", "#0a1a24", "#2a4a3a", "#d8e070"], stars: 0.6, vig: 0.8, bright: 0.95, speed: 1 },
    { id: "waves", desc: "Świetliste fale. Z muzyką delikatnie rosną.", cat: "live", name: "Fale światła", mode: 15, c: ["#04060c", "#3a8ad0", "#8ad6d0", "#b48ad6"], stars: 0, vig: 0.8, bright: 0.9, speed: 1 },
    { id: "synth", desc: "Paskowane słońce nad neonową siatką, lata 80.", cat: "live", name: "Synthwave", mode: 10, c: ["#06030c", "#2a0a3a", "#d03a8a", "#f0a050"], stars: 0.6, vig: 0.75, bright: 0.85, speed: 1 },
    { id: "lava", desc: "Kule wosku unoszą się i łączą jak w lampie.", cat: "live", name: "Lawa", mode: 4, c: ["#08050e", "#170b22", "#6a2456", "#d0784e"], stars: 0, vig: 0.85, bright: 0.85, speed: 1.3 },
    { id: "pool", desc: "Refleksy światła na dnie basenu.", cat: "live", name: "Basen", mode: 7, c: ["#03101a", "#0a2a3a", "#2f8f9a", "#8ad6d0"], stars: 0, vig: 0.85, bright: 0.85, speed: 1 },
    { id: "lights", desc: "Rozmyte światełka pływające w ciemności.", cat: "live", name: "Światła", mode: 6, c: ["#05070d", "#1d6f8a", "#8ad6d0", "#b48ad6"], stars: 0, vig: 0.8, bright: 0.9, speed: 1.2 },
    { id: "moonsea", desc: "Księżyc nad falami i jego srebrna droga na wodzie.", cat: "live", name: "Morze nocą", mode: 5, c: ["#02050b", "#050a1a", "#1a3554", "#2c4a78"], sun: [0.68, 0.3, 0.028, -0.8], stars: 0.7, vig: 0.8, bright: 0.9, speed: 0.8 },
    { id: "neon", desc: "Szybki fiolet z różem.", cat: "live", name: "Neon", mode: 0, c: ["#0a0414", "#4c1488", "#1690b8", "#f05aa8"], stars: 0, vig: 0.85, bright: 0.95, speed: 2.2 }
]

var cats = [
    { id: "auto", name: "Same się dopasowują", hint: "pora dnia · pogoda" },
    { id: "calm", name: "Spokojne", hint: "" },
    { id: "live", name: "Żywe", hint: "" },
    { id: "toon", name: "Bajki", hint: "postacie i ilustracje" }
]
function inCat(c) { return list.filter(function (p) { return p.cat === c }) }

// tempo chip in the picker -> speed multiplier
var tempos = [
    { id: "calm", name: "Spokojnie", k: 0.55 },
    { id: "normal", name: "Normalnie", k: 1 },
    { id: "fast", name: "Dynamicznie", k: 2 }
]

function get(id) {
    for (var i = 0; i < list.length; i++) if (list[i].id === id) return list[i]
    return null
}
function tempo(id) {
    for (var i = 0; i < tempos.length; i++) if (tempos[i].id === id) return tempos[i].k
    return 1
}

// palettes by sun altitude (degrees): night, blue hour, golden hour, morning, midday
var keys = [
    { alt: -18, c: ["#03060e", "#0c1736", "#1f3170", "#3a3384"], stars: 0.9 },
    { alt: -7, c: ["#060a1c", "#1a2656", "#45428a", "#b06a86"], stars: 0.35 },
    { alt: 1, c: ["#0e0812", "#2c1c48", "#b0564a", "#f0a060"], stars: 0 },
    { alt: 12, c: ["#071624", "#164a7a", "#2f86b0", "#8ad6d0"], stars: 0 },
    { alt: 35, c: ["#06121e", "#123e6a", "#2a78a6", "#9ad8e0"], stars: 0 }
]

function hex(c) { return [parseInt(c.substr(1, 2), 16) / 255, parseInt(c.substr(3, 2), 16) / 255, parseInt(c.substr(5, 2), 16) / 255] }
function css(v) {
    var s = "#"
    for (var i = 0; i < 3; i++) { var n = Math.round(Math.max(0, Math.min(1, v[i])) * 255); s += (n < 16 ? "0" : "") + n.toString(16) }
    return s
}
function mix(a, b, k) { return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k] }

// colours + stars for the sun at `alt`, dulled by clouds (0..1) and wetness (rain / snow): grey, darker, no stars
function day(alt, cloud, wet) {
    var i = 0
    while (i < keys.length - 2 && alt > keys[i + 1].alt) i++
    var a = keys[i], b = keys[i + 1]
    var k = Math.max(0, Math.min(1, (alt - a.alt) / (b.alt - a.alt)))
    k = k * k * (3 - 2 * k)
    var dull = Math.min(1, 0.55 * cloud + (wet ? 0.3 : 0))
    var c = []
    for (var j = 0; j < 4; j++) {
        var v = mix(hex(a.c[j]), hex(b.c[j]), k)
        var g = 0.3 * v[0] + 0.55 * v[1] + 0.15 * v[2]
        v = mix(v, [g * 0.95, g, g * 1.08], dull * 0.45)
        c.push(css([v[0] * (1 - 0.1 * dull), v[1] * (1 - 0.1 * dull), v[2] * (1 - 0.1 * dull)]))
    }
    return { c: c, stars: (a.stars + (b.stars - a.stars) * k) * (1 - cloud), dull: dull }
}
