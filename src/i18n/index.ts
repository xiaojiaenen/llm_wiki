import i18n from "i18next"
import { initReactI18next } from "react-i18next"
import en from "./en.json"
import zh from "./zh.json"

/** 检测浏览器语言，返回支持的语言代码（zh / en） */
function detectBrowserLanguage(): string {
  const supported = ["zh", "en"]
  // navigator.languages 是用户偏好的语言列表，navigator.language 是首选
  const candidates = navigator.languages?.length
    ? navigator.languages
    : [navigator.language]
  for (const lang of candidates) {
    const lower = lang.toLowerCase()
    // 匹配 zh、zh-cn、zh-tw、zh-hans 等
    if (lower.startsWith("zh")) return "zh"
    if (lower.startsWith("en")) return "en"
  }
  return "en"
}

i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    zh: { translation: zh },
  },
  lng: detectBrowserLanguage(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
})

export default i18n
