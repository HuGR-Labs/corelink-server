// SPDX-License-Identifier: BSD-2-Clause
'use strict'

const fs = require('fs')
const path = require('path')
const stream = require('stream')
const {promisify} = require('util')
const yauzl = require('yauzl')

const openZip = promisify(yauzl.open)
const pipeline = promisify(stream.pipeline)

function resolveInside(root, entryName) {
  const resolved = path.resolve(root, entryName)
  const relative = path.relative(root, resolved)
  if (relative.startsWith('..' + path.sep) || relative === '..' || path.isAbsolute(relative)) {
    throw new Error(`Out of bound path "${entryName}" found while processing archive`)
  }
  return resolved
}

function validateSymlinkTarget(root, destination, target) {
  if (path.isAbsolute(target)) {
    throw new Error(`Absolute symlink target is not allowed: "${target}"`)
  }
  const resolved = path.resolve(path.dirname(destination), target)
  const relative = path.relative(root, resolved)
  if (relative.startsWith('..' + path.sep) || relative === '..' || path.isAbsolute(relative)) {
    throw new Error(`Out of bound symlink target "${target}" found in archive`)
  }
  return resolved
}

function isSymlink(entry) {
  const mode = (entry.externalFileAttributes >>> 16) & 0xffff
  return (mode & 0xf000) === 0xa000
}

function isDirectory(entry) {
  const mode = (entry.externalFileAttributes >>> 16) & 0xffff
  if ((mode & 0xf000) === 0x4000 || entry.fileName.endsWith('/')) return true
  return (entry.versionMadeBy >>> 8) === 0 && entry.externalFileAttributes === 16
}

function modeFor(entry, directory) {
  let mode = (entry.externalFileAttributes >>> 16) & 0x0fff
  if (mode !== 0) return mode & 0o777
  return directory ? 0o755 : 0o644
}

async function assertSafeParent(root, parent) {
  // Validate the lexical boundary before creating anything. Then walk every
  // existing component without following links, so a pre-existing or
  // concurrently introduced symlink cannot turn recursive mkdir into an
  // escape. The post-mkdir call below repeats this check for newly-created
  // components.
  resolveInside(root, path.relative(root, parent))
  let current = parent
  while (current !== root) {
    let stats
    try {
      stats = await fs.promises.lstat(current)
    } catch (error) {
      if (error.code !== 'ENOENT') throw error
      current = path.dirname(current)
      continue
    }
    if (stats.isSymbolicLink()) {
      throw new Error(`Refusing symlink parent "${path.relative(root, current)}"`)
    }
    if (!stats.isDirectory()) {
      throw new Error(`Refusing non-directory parent "${path.relative(root, current)}"`)
    }
    break
  }
  const canonical = await fs.promises.realpath(current)
  resolveInside(root, path.relative(root, canonical))
}

async function extractEntry(zipfile, entry, root, opts) {
  const destination = resolveInside(root, entry.fileName)
  const directory = isDirectory(entry)
  const symlink = isSymlink(entry)
  if (opts.onEntry) opts.onEntry(entry, zipfile)
  const parent = directory ? destination : path.dirname(destination)
  await assertSafeParent(root, parent)
  await fs.promises.mkdir(parent, {recursive: true, mode: directory ? modeFor(entry, true) : undefined})

  // Resolve the actual parent after mkdir and prior archive entries have run.
  // This prevents a preceding archive symlink from becoming a traversal
  // primitive and catches a symlink race during recursive mkdir.
  await assertSafeParent(root, parent)
  const canonicalParent = await fs.promises.realpath(parent)
  resolveInside(root, path.relative(root, canonicalParent))

  if (directory) return
  const existing = await fs.promises.lstat(destination).catch(error => {
    if (error.code === 'ENOENT') return null
    throw error
  })
  if (existing && existing.isSymbolicLink()) {
    throw new Error(`Refusing to overwrite symlink "${entry.fileName}"`)
  }

  const readStream = await promisify(zipfile.openReadStream.bind(zipfile))(entry)
  if (symlink) {
    const target = await new Promise((resolve, reject) => {
      const chunks = []
      readStream.on('data', chunk => chunks.push(chunk))
      readStream.on('error', reject)
      readStream.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')))
    })
    validateSymlinkTarget(root, destination, target)
    await fs.promises.symlink(target, destination)
    return
  }
  await pipeline(readStream, fs.createWriteStream(destination, {mode: modeFor(entry, false), flags: 'wx'}))
}

async function extract(zipPath, opts = {}) {
  if (!opts.dir) throw new TypeError('The "dir" option is required')
  await fs.promises.mkdir(path.resolve(opts.dir), {recursive: true})
  // Compare canonical paths below. On macOS, tmpdir() and realpath() may
  // differ by a /private alias; treating the alias as an escape would reject
  // every ordinary nested file while failing to express the actual boundary.
  const root = await fs.promises.realpath(path.resolve(opts.dir))
  const zipfile = await openZip(zipPath, {lazyEntries: true})
  let canceled = false
  try {
    await new Promise((resolve, reject) => {
      zipfile.on('error', reject)
      zipfile.on('end', resolve)
      zipfile.readEntry()
      zipfile.on('entry', entry => {
        if (canceled) return
        if (entry.fileName.startsWith('__MACOSX/')) {
          zipfile.readEntry()
          return
        }
        extractEntry(zipfile, entry, root, opts)
          .then(() => zipfile.readEntry())
          .catch(error => {
            canceled = true
            zipfile.close()
            reject(error)
          })
      })
    })
  } finally {
    if (!canceled) zipfile.close()
  }
}

module.exports = extract
module.exports.resolveInside = resolveInside
module.exports.validateSymlinkTarget = validateSymlinkTarget
