# Zen Browser w stylu Mist

[English](zen.md)

`integrations/zen/` zamienia [Zen Browser](https://zen-browser.app) w okno Mist: pasek boczny to płynne szkło
nad rozmytą tapetą (jak górny pasek i dock), strona unosi się na nim jak karta z obwódką i cieniem Mist,
a każde menu, popup, panel, podpowiedź, okno dialogowe i pasek wyszukiwania wygląda jak panel Mist
(przyciemnione szkło, 1 px krawędź soczewki, miękki cień, niebieskie `#0A84FF` zaznaczenie). Pływający pasek
adresu wygląda jak Spotlight, przyciski okna to macOS-owe „światła”, a powiadomienia jak Dynamic Island.
Fonty jak w mist-ui: SF Pro, potem Inter, potem Noto Sans — w interfejsie i jako domyślny bezszeryfowy na
stronach; SF Mono lub JetBrains Mono dla kodu. Ustawienia, dodatki, about:config i nowa karta mają tę samą
paletę, zielone przełączniki i boczne menu jak w Ustawieniach systemowych.

| Plik | Co |
|---|---|
| `chrome/mist/tokens.css` | tokeny Mist (`theme.rs`, `glass.rs`) jako zmienne CSS |
| `chrome/mist/chrome.css` | interfejs: okno, pasek boczny, karty, pasek adresu, menu, panele, dialogi, elementy Zen |
| `chrome/mist/content.css` | strony about: |
| `user.js` | ustawienia: wczytanie arkuszy, przezroczystość, tryb ciemny, akcent, fonty, nakładane paski przewijania |

## Instalacja

```bash
~/.config/quickshell/mist/integrations/zen/install.sh            # wszystkie profile z profiles.ini
~/.config/quickshell/mist/integrations/zen/install.sh <profil>   # tylko jeden
```

Szuka profili w `~/.zen`, `~/.config/zen` i katalogu Flatpaka, kopiuje `chrome/` do każdego i dopisuje
ustawienia do `<profil>/user.js` między `// mist-begin` a `// mist-end`. Istniejący `userChrome.css` /
`userContent.css` zostaje zachowany jako `*.before-mist`; twoje reguły zostają, a `@import`-y Mist trafiają na
początek. Starszy, ręcznie robiony motyw Mist (plik z `MIST SHELL THEME`) zostaje zastąpiony, bo zostawiony
pod `@import`-ami nadpisywałby ten. Po aktualizacji Mist uruchom ponownie. Zrestartuj Zen.

Cofnięcie: `install.sh --uninstall`. Ustawienia zapisane już w `prefs.js` zostają, dopóki nie zresetujesz ich
w `about:config`.

## Hyprland

Pasek boczny jest półprzezroczysty; Hyprland rozmywa to, co pod nim, jeśli `decoration.blur` jest włączone
(domyślnie jest). Żeby rozmyć też menu i popupy, ustaw `decoration.blur.popups = true`.
Jeśli okno wygląda płasko, sprawdź, czy żadna reguła okna nie wymusza `opaque` dla Zen.

To, co unosi się nad stroną (pasek boczny w trybie kompaktowym, pływający pasek adresu, wyszukiwanie na
stronie, dialogi), ma pełny kolor szkła: Zen nie potrafi rozmyć strony pod spodem, a prześwitujący panel
pokazuje ostry tekst strony za kartami.

Żółte światło minimalizuje przez pozycję kliknięcia (Hyprland ignoruje prośbę aplikacji): jego środek jest
41, 20 px od lewego górnego rogu okna; `ZEN_MIN` w konfiguracji Hyprland musi się zgadzać.

## Wskazówki

- Bez SF Pro używany jest Inter Variable (`pacman -S inter-font`).
- Kolory przestrzeni roboczych są wyłączone (`zen.theme.gradient`), więc każda przestrzeń to to samo szkło.
  Usuń tę linię z `user.js`, żeby je przywrócić.
- Zen zmienia nazwy elementów między wydaniami; to, co przestanie pasować, po prostu wygląda domyślnie.
  Browser Toolbox (`Ctrl+Alt+Shift+I`) pokaże nowy selektor do poprawki w `chrome/mist/chrome.css`.
