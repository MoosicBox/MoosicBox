use hyperchad_template::container;

fn main() {
    container! {
        anchor href={ div {} } {}
    };

    container! {
        anchor href=.pinkie-pie {} {}
    };

    container! {
        anchor .{ div {} } {}
    };

    container! {
        anchor #{ div {} } {}
    };

    container! {
        @if true {
        } @else if true {
        } @else {
            anchor href={ div #if-else {} } {}
        }
    };

    container! {
        @for _ in 0..10 {
            anchor href={ div #for {} } {}
        }
    };

    container! {
        @while false {
            anchor href={ div #while {} } {}
        }
    };

    container! {
        @match () {
            () => anchor href={ div #match {} } {}
        }
    };
}
