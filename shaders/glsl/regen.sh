#!/bin/sh
# Regenerates the GLSL 150 that mist-wall loads (shaders/glsl/*.frag) from the Qt shader packs
# (shaders/*.frag.qsb, built from shaders/*.frag with qsb). Only after editing a shader; needs qt6-shadertools.
set -e
cd "$(dirname "$0")/.."
for q in *.frag.qsb; do
    /usr/lib/qt6/bin/qsb -x glsl,150 -o "glsl/${q%.qsb}" "$q"
done
