import {
  createCliRenderer,
  TextRenderable,
  BoxRenderable,
  RGBA,
  TextAttributes,
} from "@xincli/opentui-core"

const renderer = await createCliRenderer({ exitOnCtrlC: true })
renderer.start()

renderer.setBackgroundColor(RGBA.fromInts(18, 22, 35, 255))

const title = new TextRenderable(renderer, {
  id: "title",
  content: "Termux TUI",
  position: "absolute",
  left: 2,
  top: 1,
  fg: RGBA.fromInts(255, 215, 135),
  attributes: TextAttributes.BOLD,
  zIndex: 100,
})
renderer.root.add(title)

const hint = new TextRenderable(renderer, {
  id: "hint",
  content: "Press Ctrl+C or q to exit",
  position: "absolute",
  left: 2,
  top: 2,
  fg: RGBA.fromInts(176, 196, 222),
  zIndex: 100,
})
renderer.root.add(hint)

const box = new BoxRenderable(renderer, {
  id: "box",
  position: "absolute",
  left: 2,
  top: 4,
  width: 40,
  height: 5,
  border: true,
  borderStyle: "rounded",
  borderColor: RGBA.fromInts(100, 180, 200, 255),
  title: "Hello, world!",
  titleAlignment: "center",
  backgroundColor: RGBA.fromInts(30, 34, 50, 255),
  zIndex: 10,
})
renderer.root.add(box)

renderer.keyInput.on("keypress", (key) => {
  if (key.name === "q") {
    renderer.stop()
    renderer.destroy()
    process.exit(0)
  }
})
