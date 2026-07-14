import 'styled-components';
import { colors } from './theme';

type CustomTheme = {
  colors: typeof colors.light;
};

declare module 'styled-components' {
  export interface DefaultTheme extends CustomTheme {}
}
