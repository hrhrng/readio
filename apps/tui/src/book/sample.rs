//! The built-in sample book, so `readio` with no arguments still has something
//! to read. Both texts are original, written for this project.
//!
//! The sample follows the interface language: a first run that shows English
//! chrome around Chinese prose teaches the reader nothing about either.

use super::{Book, Source, text};

const EN: &str = r#"# One  Attention in eighty columns

The first time I noticed that attention has a shape, I was watching a cursor blink in an eighty-column terminal.

I was waiting on a slow build. There was nothing on the screen but that one line, breathing in place. I stared at it and realised I was not impatient — I was calm. What settled me, I understood later, was not the waiting. It was that the interface was asking me for nothing. It did not pop up, recommend, flash, or pretend to care about me. It did one thing, finished it, and told me it was done.

We tend to talk about attention as a quantity, as though it were water in a bucket: use it up and it is gone. A better comparison might be a riverbed. The volume of water is rarely the problem. The problem is the shape that has been dug for it to run through. An interface that redirects you every ten seconds carves a bed that is shallow and broken. An interface that does one thing lets the water gather and move in a single direction.

> An interface is never neutral. While it presents content, it is also shaping the way you read.

So I began to doubt something I had believed for years: perhaps I cannot finish long articles not because my patience has decayed, but because I have almost never read one in a genuinely quiet place.

# Two  The progress bar

A progress bar is a strange invention. It cuts something continuous into a length that can be measured.

In a film it tells you how long is left, which is why the last ten minutes always feel tense. In a paper book your own hand is the progress bar — the right side thins, the left side thickens — and the process is gentle and vague, never accurate to a percentage. An e-reader turns it into a number: thirty-seven per cent.

For a long time I hated that number. It turned reading into completion management: four per cent today, one per cent yesterday. I was chasing a target I had set myself and was continually failing.

Then I changed my mind, for a practical reason. Without any sense of position I would not dare start a thick book at all. Progress is not the oppressive part; uncertainty is. What wears you down is not "there is a lot left" but "I have no idea how much is left".

The problem was never the bar. It was where the bar had been put. In the centre of the screen it steals attention. In a corner, appearing only when you go looking for it, the same number becomes reassurance.

Most arguments in interface design come down to this: identical information, placed differently, becomes a different thing.

# Three  The feel of a tool

Good tools have a feel. Feel is hard to define and easy to recognise.

A good knife carries its weight where you need it. A good editor puts no perceptible delay between the key you press and the character that appears. Feel is made of countless five-millisecond and three-pixel decisions, none of which is worth discussing alone, and which together decide whether you are willing to use the thing every day.

I have seen a great deal of software that does everything and that nobody wants to touch, and software with very few features that people have used for ten years. The difference is rarely capability. It is whether using it feels smooth — and smooth means there is no wall of translation standing between your intention and the program's response.

Terminal programs have a natural advantage here. Their feedback loop is extremely short, and they do not try to guess what you want. You type a command and it runs. You type it wrong and it says so. That relationship is clear, and clarity is itself a kind of comfort.

# Four  One continuous piece of work

The interfaces I like best share one property: they make you feel you are doing a single continuous piece of work, rather than handling a series of interrupted fragments.

Continuity is fragile. One unnecessary confirmation dialog, one layout jump, one five-hundred-millisecond blank is enough to break it. And once it breaks, the cost is not the few seconds you waited. It is the price of getting back into the state, which is usually several minutes.

So when I judge a tool now, I ask one question: after an hour with it, am I more tired, or more focused?

The answer rarely has anything to do with the feature list.
"#;

const ZH: &str = r#"# 一 终端里的注意力

我第一次意识到注意力有形状，是在一个只有八十列宽的终端里。

那天我在等一个很慢的编译。屏幕上什么都没有，只有一行光标在原地呼吸。我盯着它，忽然发现自己并不焦躁——相反，我很安静。后来我才明白，让我安静下来的不是等待本身，而是这个界面没有向我索取任何东西。它不弹窗，不推荐，不闪烁，也不假装关心我。它只是把一件事做完，然后告诉我做完了。

我们习惯把注意力当成一种容量，好像它是水桶里的水，用完就没有了。但更准确的比喻也许是河道。水的总量并不总是问题，问题是河床被挖成了什么形状。一个每隔十秒就把你引向别处的界面，会把你的河道挖得又浅又碎；而一个只做一件事的界面，会让水重新聚起来，往一个方向流。

> 界面不是中立的。它一边呈现内容，一边塑造你阅读内容的方式。

所以我开始怀疑一件事：我读不下去长文章，可能并不是因为我的耐心变差了，而是因为我几乎没有在一个真正安静的地方读过长文章。

# 二 阅读的进度条

进度条是一个奇怪的发明。它把一件本来连续的事切成了可以被丈量的段落。

看电影时，进度条会告诉你还剩多久，于是你在最后十分钟总是紧张的。读纸书时，你的手指本身就是进度条——右手越来越薄，左手越来越厚，这个过程温和、模糊，不会精确到百分比。而电子阅读器把它变成了一个数字：百分之三十七。

我很长一段时间讨厌这个数字。它让阅读变成了一种完成度管理：今天推进了百分之四，昨天只有百分之一。我在追赶一个由我自己设定、又不断让我失望的指标。

但后来我改了主意，原因很实际：如果没有任何位置感，我根本不敢开始读一本厚书。进度不是压迫，不确定才是。真正折磨人的不是"还剩很多",而是"我不知道还剩多少"。

问题不在进度条，而在它被摆在哪里。放在正中央，它会抢走注意力；放在角落里，只有当你主动去找的时候它才出现，那它就变成了一种安慰。

界面设计里很多争论，最后都归结为这一件事：同样的信息，放在不同的位置，会变成完全不同的东西。

# 三 工具的手感

好工具都有手感。手感很难定义，但很容易分辨。

一把好用的刀，重量落在你需要的地方；一个好用的编辑器，你按下按键和字符出现之间没有可感知的延迟。手感是这样一种东西：它由无数个五毫秒和三像素组成，任何单独一项都不值得讨论，合在一起却决定了你愿不愿意每天用它。

我见过很多功能齐全但没人愿意用的软件，也见过功能很少却被人用了十年的软件。差别常常不在能力，而在它是否让你觉得"顺"。顺意味着：你的意图和它的反应之间，没有一道需要翻译的墙。

终端程序在这件事上有天然优势：它的反馈周期极短，而且它不试图猜测你想要什么。你打一个命令，它执行；你打错了，它报错。这种关系是清楚的，而清楚本身就是一种舒适。

# 四 一次连续的工作

我最喜欢的界面，都有一个共同点：它们让你觉得自己正在进行一次连续的工作，而不是在处理一连串被打断的片段。

连续感是很脆弱的。一次不必要的确认弹窗、一次布局跳动、一次五百毫秒的空白，都足以把它打断。而一旦被打断，你需要付出的不只是那几秒钟，还有重新回到状态里的成本——那个成本往往是几分钟。

所以我现在评价一个工具，只问一个问题：用它一小时之后，我是更累了，还是更专注了？

答案往往和功能列表无关。
"#;

pub fn book() -> Book {
    let chinese = matches!(crate::i18n::current(), crate::i18n::Lang::Zh);
    let (text, title, author) = if chinese {
        (ZH, "注意力的形状", "readio 示例")
    } else {
        (EN, "The Shape of Attention", "readio sample")
    };
    let chapters = text::parse(text, "attention.md");
    Book {
        // One identity regardless of language: switching the interface should
        // not orphan the progress the reader already made.
        id: super::synthetic_id("readio://sample"),
        title: title.to_string(),
        author: Some(author.to_string()),
        path: None,
        source: Source::Sample,
        chapters,
        cover: None,
    }
}
