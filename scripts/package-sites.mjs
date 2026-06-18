import { cp, mkdir, rm } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const uiDist = join(root, 'study-engine-ui', 'dist')
const out = join(root, 'dist')
const clientOut = join(out, 'client')

await rm(out, { recursive: true, force: true })
await mkdir(clientOut, { recursive: true })
await mkdir(join(out, 'server'), { recursive: true })
await mkdir(join(out, '.openai'), { recursive: true })

await cp(uiDist, clientOut, { recursive: true })
await cp(join(root, 'study-engine-ui', 'worker', 'index.js'), join(out, 'server', 'index.js'), {
  recursive: true
})
await cp(join(root, '.openai', 'hosting.json'), join(out, '.openai', 'hosting.json'), {
  recursive: true
})

const builtinQuestionFiles = ['cca-f.json']
await mkdir(join(clientOut, 'questions'), { recursive: true })
for (const file of builtinQuestionFiles) {
  await cp(join(root, 'questions', file), join(clientOut, 'questions', file))
}
