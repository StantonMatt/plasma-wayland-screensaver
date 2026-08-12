.pragma library

function colors(name) {
    switch (name) {
    case "spectrum": return ["#ff477e", "#ffbe0b", "#42e2b8", "#3a86ff", "#b967ff", "#fb5607"]
    case "ember": return ["#fff1a8", "#ffc857", "#ff7b42", "#ef3e36", "#9c1c28", "#ffd6a5"]
    case "forest": return ["#d8f3dc", "#95d5b2", "#52b788", "#2d6a4f", "#b7e4c7", "#74c69d"]
    case "mono": return ["#ffffff", "#d9e1e8", "#aeb8c2", "#7f8b96", "#edf2f4", "#bac4ce"]
    case "pastel": return ["#ffc8dd", "#bde0fe", "#caffbf", "#ffd6a5", "#e7c6ff", "#a2d2ff"]
    default: return ["#d9fbff", "#3dd6e8", "#3a86ff", "#7358d6", "#2aa889", "#9bf6ff"]
    }
}

function fract(value) {
    return value - Math.floor(value)
}

function random(index, seed) {
    return fract(Math.sin(index * 127.1 + seed * 311.7) * 43758.5453123)
}

function positiveModulo(value, modulus) {
    return modulus > 0 ? ((value % modulus) + modulus) % modulus : 0
}
