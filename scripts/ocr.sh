#!/bin/bash
# Text from the screen: select a region, tesseract reads it, the text goes to the clipboard.
# Languages: $MIST_OCR_LANG, else every installed one of pol + eng.
say() { mist-ui-bin msg island "$@" >/dev/null 2>&1; }
command -v tesseract >/dev/null || { say show alert "Brak tesseract" "zainstaluj tesseract i tesseract-data-pol"; exit 1; }
g=$(slurp -d -b '#00000040' -c '#8ad6d0' -s '#ffffff15' -w 1 2>/dev/null) || exit 0
[ -n "$g" ] || exit 0
langs=$(tesseract --list-langs 2>/dev/null | grep -xE 'pol|eng' | paste -sd+)
txt=$(grim -g "$g" - | tesseract - - -l "${MIST_OCR_LANG:-${langs:-eng}}" 2>/dev/null | sed -e 's/[[:space:]]*$//' | sed -e '/./,$!d')
if [ -z "$txt" ]; then
    say show textScan "Nie znaleziono tekstu" ""
    exit 0
fi
printf '%s' "$txt" | wl-copy
say show textScan "Tekst w schowku" "$(printf '%s' "$txt" | head -c 80 | tr '\n' ' ')"
