// SPDX-License-Identifier: MIT
'use strict'

const MAX_INPUT_BYTES = 10 * 1024 * 1024

function fail(format) {
  throw new TypeError(`Unsupported or unsafe image format${format ? `: ${format}` : ''}`)
}

function png(input) {
  if (input.length < 24 || !input.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) return null
  return {width: input.readUInt32BE(16), height: input.readUInt32BE(20), type: 'png'}
}

function svg(input) {
  const text = input.subarray(0, 1024 * 1024).toString('utf8').replace(/^\uFEFF/, '')
  const tag = text.match(/<svg\b[^>]*>/i)
  if (!tag) return null
  const value = (name) => {
    const found = tag[0].match(new RegExp(`\\b${name}\\s*=\\s*["']([^"']+)["']`, 'i'))
    if (!found) return undefined
    const number = found[1].trim().match(/^([0-9]+(?:\.[0-9]+)?)(?:px)?$/i)
    return number ? Number(number[1]) : undefined
  }
  let width = value('width')
  let height = value('height')
  const viewBox = tag[0].match(/\bviewBox\s*=\s*["']\s*([0-9.+-]+)[ ,]+([0-9.+-]+)[ ,]+([0-9.+-]+)[ ,]+([0-9.+-]+)\s*["']/i)
  if (viewBox) {
    width ??= Number(viewBox[3])
    height ??= Number(viewBox[4])
  }
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return null
  return {width, height, type: 'svg'}
}

function imageSize(input) {
  if (!Buffer.isBuffer(input) && !(input instanceof Uint8Array)) throw new TypeError('image input must be bytes')
  const bytes = Buffer.from(input)
  if (bytes.length > MAX_INPUT_BYTES) fail('input exceeds 10 MiB')
  return png(bytes) || svg(bytes) || fail()
}

imageSize.MAX_INPUT_BYTES = MAX_INPUT_BYTES
module.exports = imageSize
module.exports.imageSize = imageSize
