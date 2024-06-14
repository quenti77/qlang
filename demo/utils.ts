import { bgGreen } from 'ansicolor'

import prompts from 'prompts'

export function header(title: string) {
    console.log(' ')
    console.log(bgGreen(' '.repeat(title.length + 10)))
    console.log(bgGreen(`     ${title}     `))
    console.log(bgGreen(' '.repeat(title.length + 10)))
    console.log('\n')
}

export async function prompt(message: string) {
    const response = await prompts({
        type: 'text',
        name: 'command',
        message,
    })

    return response.command
}

export async function loopCommand(callback: (code: string) => void) {
    let continueLoop = true

    while (continueLoop) {
        const code = await prompt('>>>')

        if (code === 'exit' || code === 'quit' || code === '') {
            continueLoop = false
            continue
        }

        callback(code)
    }

    console.log('\nGoodbye!')
}
