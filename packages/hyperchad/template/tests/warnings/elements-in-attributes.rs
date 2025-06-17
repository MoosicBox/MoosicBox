use hyperchad_template::container;

fn main() {
    container! {
        a href={ b {} } {}
    };

    container! {
        a href=.pinkie-pie {} {}
    };

    container! {
        a .{ b {} } {}
    };

    container! {
        a #{ b {} } {}
    };

    container! {
        @if true {
        } @else if true {
        } @else {
            a href={ b #if-else {} } {}
        }
    };

    container! {
        @for _ in 0..10 {
            a href={ b #for {} } {}
        }
    };

    container! {
        @while false {
            a href={ b #while {} } {}
        }
    };

    container! {
        @match () {
            () => a href={ b #match {} } {}
        }
    };
}
