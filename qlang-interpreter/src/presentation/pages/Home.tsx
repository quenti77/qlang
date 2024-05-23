import CodeTile from "@/features/home/components/CodeTile"
import HelpTile from "@/features/home/components/HelpTile"
import OutputTile, { OutputRow } from "@/features/home/components/OutputTile"
import { run } from "@/infrastructure/monacoEditor/languages/qlang"
import { debounce } from "@/infrastructure/utils"
import { useState } from "react"

export default function Home() {
    const [code, setCode] = useState<string>("")
    const [output, setOutput] = useState<OutputRow[]>([])

    const handleContentChange = debounce((content: string) => {
        setCode(content)

        const { out, err } = run(content)
        const standardOutput = out.map((line) => ({ type: 'out', content: line }))
        const standardError = err.map((line) => ({ type: 'err', content: line }))

        // setOutput((prev) => [...prev, ...standardOutput, ...standardError] as OutputRow[])
        setOutput([...standardOutput, ...standardError] as OutputRow[])
    }, 500)

    return (
        <div className="m-2 grid grid-rows-3 grid-cols-4 gap-2 h-[calc(100vh-4rem)]">
            <div className="hidden lg:block lg:row-span-2 col-span-2 xl:col-span-1">
                <HelpTile />
            </div>
            <div className="row-span-2 col-span-4 lg:col-span-2 xl:col-span-3">
                <CodeTile defaultValue={code} contentChangeHandler={handleContentChange} />
            </div>
            <div className="row-start-3 row-span-1 col-span-4">
                <OutputTile output={output} />
            </div>
        </div>
    )
}
