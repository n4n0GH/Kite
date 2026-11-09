
// Reader view: appended to Readability.js, inside the function wrapping both
// (ReaderViewWindow.qml), and run in a world of its own. Readability changes
// the document it is given, hence the copy. Kite sanitises the result
// (kite_core::reader).
const article = new Readability(document.cloneNode(true)).parse();
if (!article || !article.content) return "";
return JSON.stringify({
    title: article.title || document.title,
    byline: article.byline || "",
    siteName: article.siteName || "",
    lang: article.lang || document.documentElement.lang || "",
    dir: article.dir || "",
    content: article.content,
    length: article.length || 0,
});
