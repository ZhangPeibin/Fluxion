// === 第一步：struct 是「自定义数据类型」，就是一个装数据的盒子 ===
struct Dog;
struct Cat;
struct Cow;

// === 第二步：trait 是「一份契约 / 规范」，规定"会做什么"，不管"怎么做" ===
// 这里规定：凡是遵守 Animal 契约的类型，都必须能 speak()，返回一句话
trait Animal {
    fn speak(&self) -> String;
}

// === 第三步：让某个类型「遵守契约」，就是 impl Trait for Type，并写出具体怎么做 ===
impl Animal for Dog {
    fn speak(&self) -> String {
        "汪汪".to_string()
    }
}

impl Animal for Cat {
    fn speak(&self) -> String {
        "喵".to_string()
    }
}

impl Animal for Cow {
    fn speak(&self) -> String {
        "哞".to_string()
    }
}

// === 第四步：写一个只认「契约」、不认具体类型的函数 ===
// 参数是 &dyn Animal：意思是"随便什么，只要它遵守 Animal 契约"
fn make_it_speak(animal: &dyn Animal) {
    println!("{}", animal.speak());
}

fn main() {
    let dog = Dog;
    let cat = Cat;

    make_it_speak(&dog); // 传狗
    make_it_speak(&cat); // 传猫
    make_it_speak(&Cow); // 传牛

    // 把不同具体类型放进同一个容器，统一调用 —— 这就是 trait 的威力
    let animals: Vec<Box<dyn Animal>> = vec![Box::new(Dog), Box::new(Cat), Box::new(Cow)];
    for a in &animals {
        println!("{}", a.speak());
    }
}
