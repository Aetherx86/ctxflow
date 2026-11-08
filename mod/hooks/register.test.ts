import { expect, test } from 'claude-code/testing'

function stubs(on: any, advise: () => string, calls: string[][]) {
  on('process.run', (_: unknown, e: any) => {
    const argv = e.argv ?? e
    calls.push(argv)
    return { value: { exitCode: 0, stdout: argv[1] === 'advise' ? advise() : 'auto · advisor 0/3', stderr: '' } }
  })
  on('ui.status', () => ({ value: undefined }))
  on('ui.toast', () => ({ value: undefined }))
}

test('a successful command never calls ctxflow', async ($, on) => {
  const calls: string[][] = []
  stubs(on, () => 'should not run', calls)
  on('tool.call', () => ({ result: {} }))
  const r = await $.tool.call({ tool: 'Bash', command: 'ls' })
  expect(calls.length).toBe(0)
  expect(r.context).toBeUndefined()
})

test('a failed command asks the gate; silence adds nothing to the context', async ($, on) => {
  const calls: string[][] = []
  stubs(on, () => '', calls)
  on('tool.call', () => ({ result: {}, isError: true, text: 'boom' }))
  const r = await $.tool.call({ tool: 'Bash', command: 'make' })
  expect(calls[0].slice(0, 6)).toEqual(['ctxflow', 'advise', 'error', '--auto', '--stdin', '--'])
  expect(r.context).toBeUndefined()
})

test('advice reaches the model when the gate opens', async ($, on) => {
  stubs(on, () => 'CHANGE: check the input', [])
  on('tool.call', () => ({ result: {}, isError: true, text: 'boom' }))
  const r = await $.tool.call({ tool: 'Bash', command: 'make' })
  expect(r.context?.[0]).toContain('CHANGE: check the input')
})
