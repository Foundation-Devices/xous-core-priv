// Copyright (c) 2022 Sam Blenny
// SPDX-License-Identifier: Apache-2.0 OR MIT
//
package lib

import (
	"bytes"
	"strings"
	"text/template"
)

type FontSummary struct {
	Name string
	Len  int
}
type FontMap struct {
	Name string
	Len  string
}

// Render the command line usage message
func RenderUsageTemplate(confirm string, debug string, fonts []FontSpec) string {
	context := usageTemplateContext{confirm, debug, fonts}
	return renderTemplate(usageTemplate, "usage", context)
}

// Render rust source code for font file with index functions and static arrays
func RenderFontFileTemplate(f FontSpec, gs GlyphSet) string {
	fname := strings.ToUpper(f.Name)
	context := fontFileTemplateContext{f, gs, fname}
	return renderTemplate(fontFileTemplate, "fontfile", context)
}
func RenderLoaderFileTemplate(f FontSpec, gs GlyphSet) string {
	fname := strings.ToUpper(f.Name)
	context := loaderFileTemplateContext{f, gs, fname}
	return renderTemplate(loaderFileTemplate, "loaderfile", context)
}
func RenderFontmapTemplate(fd []FontMap) string {
	context := fontmapTemplateContext{fd}
	return renderTemplate(fontmapTemplate, "fontmap", context)
}
func RenderLoadermodTemplate(fd []FontSummary) string {
	context := loadermodTemplateContext{fd}
	return renderTemplate(loadermodTemplate, "loadermod", context)
}

// Holds data for rendering usageTemplate
type usageTemplateContext struct {
	Confirm string
	Debug   string
	Fonts   []FontSpec
}

// Holds data for rendering fontFileTemplate
type fontFileTemplateContext struct {
	Font     FontSpec
	GS       GlyphSet
	FontName string
}

// Holds data for rendering loaderFileTemplate
type loaderFileTemplateContext struct {
	Font     FontSpec
	GS       GlyphSet
	FontName string
}

// Holds data for rendering fontmapTemplate
type fontmapTemplateContext struct {
	FontDir []FontMap
}
type loadermodTemplateContext struct {
	FontDir []FontSummary
}

// Return a string from rendering the given template and context data
func renderTemplate(templateString string, name string, context interface{}) string {
	fmap := template.FuncMap{"ToLower": strings.ToLower}
	t := template.Must(template.New(name).Funcs(fmap).Parse(templateString))
	var buf bytes.Buffer
	err := t.Execute(&buf, context)
	if err != nil {
		panic(err)
	}
	return buf.String()
}

// Template with usage instructions for the command line tool
const usageTemplate = `
This tool generates fonts in the form of rust source code.
To confirm that you want to write the files, use the {{.Confirm}} switch.
To show debug info, use the {{.Debug}} switch.

Font files that will be generated:{{range $f := .Fonts}}
  {{$f.RustOut}}
  {{$f.LoaderOut}}{{end}}

Usage:
    go run main.go {{.Confirm}}
`

// Template with rust source code for a outer structure of a font file
const fontFileTemplate = `// DO NOT MAKE EDITS HERE because this file is automatically generated.
// To make changes, see <xous_root>/services/graphics-server/src/blitstr2/codegen/main.go
//
{{.Font.Legal}}
//! {{.Font.Name}} Font
#![allow(dead_code)]

/// Maximum height of glyph patterns in this bitmap typeface.
pub const MAX_HEIGHT: u8 = {{.Font.Size}};

/// Unicode character codepoints corresponding to glyph sprites in GLYPHS array.
/// Indended use:
///  1. Do binary search on CODEPOINTS to find index of the codepoint corresponding
///     to the glyph you want to locate
///  2. Multiply resulting CODEPOINTS index by 8 (<<3) to get index into GLYPHS for
///     the corresponding glyph sprite (because 16*16px sprite size is 8*u32)
pub const CODEPOINTS: [u32; {{.GS.CodepointsLen}}] = [
{{.GS.Codepoints}}];

#[cfg(any(target_os = "none", target_os = "xous"))]
pub(crate) static GLYPH_LOCATION: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
pub(crate) const GLYPH_LEN: usize = {{.GS.GlyphsLen}};

pub(crate) fn glyphs() -> &'static [u32] {
    #[cfg(any(target_os = "none", target_os = "xous"))]
    unsafe {
        let data: *const u32 = core::mem::transmute(GLYPH_LOCATION.load(core::sync::atomic::Ordering::SeqCst));
        core::slice::from_raw_parts(data, GLYPH_LEN)
    }

    #[cfg(not(any(target_os = "none", target_os = "xous")))]
    &GLYPHS
}

#[cfg(not(any(target_os = "none", target_os = "xous")))]
/// Packed 16px * 16px glyph pattern data.
/// Pixels are packed in row-major order with LSB of first pixel word
/// containing the top left pixel. Bit of 0 means clear, 1 means set
pub const GLYPHS: [u32; {{.GS.GlyphsLen}}] = [
{{.GS.Glyphs}}];
{{if .GS.Widths}}
/// Widths for proportional glyphs
pub const WIDTHS: [u8; {{.GS.WidthsLen}}] = [
{{.GS.Widths}}];
{{end}}`

const loaderFileTemplate = `
// DO NOT MAKE EDITS HERE because this file is automatically generated.
// To make changes, see <xous_root>/services/graphics-server/src/blitstr2/codegen/main.go

{{.Font.Legal}}
//! {{.Font.Name}} Font
#![allow(dead_code)]
#[link_section=".fontdata"]
#[no_mangle]
#[used]
/// Packed 16px * 16px glyph pattern data.
/// Pixels are packed in row-major order with LSB of first pixel word
/// containing the top left pixel. Bit of 0 means clear, 1 means set
pub static {{.FontName}}_GLYPHS: [u32; {{.GS.GlyphsLen}}] = [
{{.GS.Glyphs}}];
{{if .GS.Widths}}
{{end}}`

const loadermodTemplate = `// DO NOT MAKE EDITS HERE because this file is automatically generated.
// The order of these modules affects the link order in the loader, which is referred to in the graphics engine.
// To make changes, see <xous_root>/services/graphics-server/src/blitstr2/codegen/main.go

{{range $f := .FontDir}}pub mod {{$f.Name}};
{{end}}`

const fontmapTemplate = `// DO NOT MAKE EDITS HERE because this file is automatically generated.
// The order of these modules affects the link order in the loader, which is referred to in the graphics engine.
// To make changes, see <xous_root>/services/graphics-server/src/blitstr2/codegen/main.go
#![allow(dead_code)]
{{range $f := .FontDir}}pub const {{$f.Name}}: usize = 0x{{$f.Len}};
{{end}}`