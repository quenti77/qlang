import { KEYWORDS } from '@/qlang/token'
import hljs from 'highlight.js/lib/core'

hljs.registerLanguage('qlang', function () {
    return {
        keywords: Array.from(Object.keys(KEYWORDS)),
        contains: [
            {
                className: 'comment',
                begin: 'rem',
                end: '$',
            },
            {
                className: 'string',
                begin: '"',
                end: '"',
            },
        ],
    }
})
